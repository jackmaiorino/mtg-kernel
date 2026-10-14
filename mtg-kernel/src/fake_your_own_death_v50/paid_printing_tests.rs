use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::{GameState, StackItemKind, Target, Zone};

fn paid(actor: PlayerId, owner: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
        put(&mut state, player, "Mountain", Zone::Battlefield);
    }
    let target = put(&mut state, owner, "Llanowar Elves", Zone::Battlefield);
    paid_on(state, actor, target)
}

fn paid_on(
    mut state: GameState,
    actor: PlayerId,
    target: ObjectId,
) -> (GameState, ObjectId, ObjectId) {
    let spell = put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    let definition = state.objects.get(spell).card_def;
    assert_eq!(
        CARD_DEFS[usize::from(definition)].capability,
        crate::card_def::CardCapability::Partial
    );
    assert!(spell_custody::selected_definition(definition));
    assert!(crate::card_def::preflight_fully_supported_deck(&[definition]).is_err());
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
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()],
        0
    );
    assert_eq!(state.stack.last().unwrap().source, spell);
    assert!(state.engine.event_history.iter().any(|event|
        matches!(event, CommittedEvent::FakeJournalV50 { journal } if matches!(journal.as_ref(), spell_custody::JournalV50::Published { item } if item.source == spell))));
    (state, spell, target)
}

fn drain(state: &mut GameState) {
    for _ in 0..24 {
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
            return;
        }
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty() && state.engine.pending_triggers.is_empty() {
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
fn actual_partial_printing_pays_1b_and_returns_once_with_death_controller_treasure_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            let (mut state, spell, target) = paid(actor, owner);
            drain(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(engine::effective_power(&state, target), 3);
            assert_eq!(spell_custody::installed_grants(&state).unwrap().len(), 1);
            death(&mut state, target);
            let generation = state.objects.get(target).zone_change_count;
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            for game in [&mut state, &mut restored] {
                drain(game);
                spell_custody::validate_retained_grant_history(game).unwrap();
                let returned = game.objects.get(target);
                assert_eq!(
                    (
                        returned.zone,
                        returned.owner,
                        returned.controller,
                        returned.tapped
                    ),
                    (Zone::Battlefield, owner, owner, true)
                );
                assert_eq!(returned.zone_change_count, generation + 1);
                assert_eq!(engine::effective_power(game, target), 1);
                assert_eq!(treasures(game, owner), 1);
                assert_eq!(treasures(game, owner.opponent()), 0);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_paid_spell_and_gained_trigger_counter_have_distinct_no_grant_and_no_return_outcomes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut spell_countered, spell, target) = paid(actor, actor);
        let id = spell_countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut spell_countered, id).unwrap();
        assert_eq!(spell_countered.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(engine::effective_power(&spell_countered, target), 1);
        assert!(spell_custody::installed_grants(&spell_countered)
            .unwrap()
            .is_empty());
        spell_custody::validate_retained_grant_history(&spell_countered).unwrap();

        let (mut trigger_countered, _, target) = paid(actor, actor);
        drain(&mut trigger_countered);
        death(&mut trigger_countered, target);
        let id = trigger_countered.stack.last().unwrap().v4.stack_item_id;
        engine::counter_stack_item_by_id(&mut trigger_countered, id).unwrap();
        spell_custody::validate_retained_grant_history(&trigger_countered).unwrap();
        assert_eq!(trigger_countered.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(treasures(&trigger_countered, actor), 0);
    }
}

#[test]
fn actual_removed_resolution_revalidates_capture_and_collection_before_any_write() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for capture in [true, false] {
            let (mut state, _, target) = paid(actor, actor);
            drain(&mut state);
            death(&mut state, target);
            let item = state.stack.last().unwrap().clone();
            let owner = death_custody::acquire_resolution(&state, &item)
                .unwrap()
                .unwrap();
            assert_eq!(state.stack.pop().unwrap(), item);
            let index = state
                .engine
                .event_history
                .iter()
                .position(|event| {
                    matches!(event,
                    CommittedEvent::FakeJournalV50 { journal } if if capture {
                        matches!(journal.as_ref(), spell_custody::JournalV50::DeathCaptured { .. })
                    } else {
                        matches!(journal.as_ref(), spell_custody::JournalV50::DeathCollected { .. })
                    })
                })
                .unwrap();
            state.engine.event_history[index] = CommittedEvent::Tap { object: target };
            let mut context = ExecCtx::no_targets(item.source, item.controller);
            context.stack_item_id = Some(item.v4.stack_item_id);
            context.ability_source_contract = item.v4.ability_source_contract;
            let before = serde_json::to_vec(&state).unwrap();
            assert!(death_custody::resolve_owned(&mut state, owner, &item, &context).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}

#[test]
fn actual_paid_fake_banner_return_borrows_original_color_row_and_restores_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for player in [PlayerId::P0, PlayerId::P1] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
            put(&mut state, player, "Mountain", Zone::Battlefield);
        }
        let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
        for _ in 0..3 {
            let swamp = put(&mut state, actor, "Swamp", Zone::Battlefield);
            engine::advance_until_decision(&mut state);
            engine::step(&mut state, Action::ActivateManaAbility(swamp)).unwrap();
        }
        engine::step(&mut state, Action::CastSpell(banner)).unwrap();
        reach_banner_color(&mut state, actor);
        engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
        assert_eq!(
            crate::heraldic_banner_v1::authenticated_color(&state, banner).unwrap(),
            ManaColor::G
        );

        // Exercise the existing targeted animation primitive with captured source
        // and target contracts. This is an operand fixture, not a paid Tough
        // Cookie printing or Full-card admission.
        let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let mut context = ExecCtx::no_targets(source, actor);
        context.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
            &state, source,
        ));
        context.targets.push(Target::Object(banner));
        context
            .target_contracts
            .push(crate::state::StackTargetContractV4::capture(
                &state,
                Target::Object(banner),
            ));
        crate::standard_creatures_v1::execute(
            crate::standard_creatures_v1::CreatureEffectV1::ToughCookieAnimate,
            &context,
            &mut state,
        );
        assert!(engine::object_has_type(
            &state,
            banner,
            crate::card_def::CardType::Creature
        ));
        let (mut state, _, target) = paid_on(state, actor, banner);
        drain(&mut state);
        death(&mut state, target);
        let original = state.stack.last().unwrap().clone();
        reach_banner_color(&mut state, actor);
        assert_eq!(state.stack.last(), Some(&original));
        assert_eq!(state.objects.get(banner).zone, Zone::Graveyard);
        assert_eq!(treasures(&state, actor), 0);
        spell_custody::validate_retained_grant_history(&state).unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
        for game in [&mut state, &mut restored] {
            let before = serde_json::to_vec(game).unwrap();
            assert!(engine::step(game, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(game).unwrap(), before);
            engine::step(game, Action::ChooseEffectOption(2)).unwrap();
            drain(game);
            assert_eq!(game.objects.get(banner).zone, Zone::Battlefield);
            assert!(game.objects.get(banner).tapped);
            assert!(!engine::object_has_type(
                game,
                banner,
                crate::card_def::CardType::Creature
            ));
            assert_eq!(
                crate::heraldic_banner_v1::authenticated_color(game, banner).unwrap(),
                ManaColor::B
            );
            assert_eq!(treasures(game, actor), 1);
            assert!(!game
                .stack
                .iter()
                .any(|row| row.v4.stack_item_id == original.v4.stack_item_id));
            spell_custody::validate_retained_grant_history(game).unwrap();
            let mut reintroduced = game.clone();
            reintroduced.stack.push(original.clone());
            assert!(spell_custody::validate_retained_grant_history(&reintroduced).is_err());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        for kind in 0..4 {
            let mut forged: GameState = serde_json::from_slice(&bytes).unwrap();
            if kind == 0 {
                forged.stack.last_mut().unwrap().controller = actor.opponent();
            } else if kind == 1 {
                forged.stack.push(original.clone());
            } else if kind == 2 {
                forged.engine.linked_aura_operation_v1 = Default::default();
            } else {
                forged.stack.clear();
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::ChooseEffectOption(2)).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}

fn reach_banner_color(state: &mut GameState, actor: PlayerId) {
    for _ in 0..24 {
        match engine::advance_until_decision(state) {
            Decision::ChooseEntryColorV36 { player, colors, .. } => {
                assert_eq!(player, actor);
                assert_eq!(colors, crate::heraldic_banner_v1::COLORS);
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("actual Banner color choice missing: {other:?}"),
        }
    }
    panic!("actual Banner color choice did not arrive")
}
