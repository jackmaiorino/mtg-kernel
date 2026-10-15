//! Actual paid printing combinations. Initial basic lands and spare hand/library
//! cards are setup operands; no counters, damage, triggers or journals are forged.
use crate::{
    card_def::{card_id_by_name, CARD_DEFS},
    drake_hatcher_v1::custody::{self as drake, JournalV1, RemovalV1},
    engine::{self, Action, Decision},
    event::CommittedEvent,
    ids::{ObjectId, PlayerId, StackItemId},
    mana::{ManaColor, Pip},
    policy_observation_v6::tests::{advance_to_next_main_for, put},
    ravenous_amulet_v1::{OperationV1, RemovalKindV1},
    state::{GameState, StackItemKind, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    // Retain real legal actions at priority boundaries. The constructor owns
    // the starting player and the real forty-card libraries for both seats.
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Hand);
    }
    state
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(original: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_vec(original).unwrap(),
        serde_json::to_vec(replay).unwrap()
    );
}

fn validate(state: &GameState) {
    assert!(state.engine.halted.is_none());
    super::validate_token_journals(state).unwrap();
    drake::validate(state).unwrap();
}

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("actual actor priority absent");
}

fn finish(state: &mut GameState) {
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        match decision {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty()
                    && state.engine.pending_cast.is_none()
                    && state.engine.pending_activation.is_none()
                    && state.engine.pending_effect.is_none()
                    && state.engine.pending_triggers.is_empty() =>
            {
                validate(state);
                return;
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("actual paid coexistence work: {other:?}"),
        }
    }
    panic!("actual work did not finish");
}

fn mana(state: &mut GameState, actor: PlayerId, colors: &[ManaColor]) {
    for color in colors {
        let name = match color {
            ManaColor::W => "Plains",
            ManaColor::U => "Island",
            ManaColor::B => "Swamp",
            ManaColor::R => "Mountain",
            ManaColor::G => "Forest",
            ManaColor::C => panic!("fixture uses actual colored basic mana"),
        };
        let land = put(state, actor, name, Zone::Battlefield);
        priority(state, actor);
        engine::step(state, Action::ActivateManaAbility(land)).unwrap();
        assert!(state.objects.get(land).tapped);
    }
}

fn pool(state: &GameState, actor: PlayerId) -> u32 {
    state.players[actor.index()]
        .mana_pool
        .iter()
        .map(|amount| u32::from(*amount))
        .sum()
}

fn cast(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    let object = put(state, actor, name, Zone::Hand);
    let d = &CARD_DEFS[usize::from(state.objects.get(object).card_def)];
    let mut colors = vec![ManaColor::G; usize::from(d.cost.generic)];
    for pip in d.cost.pips {
        let Pip::Colored(color) = pip else {
            panic!("fixture requires the immutable ordinary printed mana cost")
        };
        colors.push(*color);
    }
    assert_eq!(pool(state, actor), 0);
    mana(state, actor, &colors);
    assert_eq!(pool(state, actor), colors.len() as u32);
    engine::step(state, Action::CastSpell(object)).unwrap();
    let decision = engine::advance_until_decision(state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "{decision:?}"
    );
    assert!(state.engine.pending_cast.is_none());
    assert!(state.stack.iter().any(|row| {
        row.source == object && row.controller == actor && row.kind == StackItemKind::Spell
    }));
    assert_eq!(pool(state, actor), 0);
    finish(state);
    assert_eq!(
        state.objects.get(object).zone,
        if d.types.contains(&crate::card_def::CardType::Instant) {
            Zone::Graveyard
        } else {
            Zone::Battlefield
        }
    );
    object
}

fn paid_activation(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    slot: u8,
) -> StackItemId {
    priority(state, actor);
    engine::step(state, Action::ActivateAbility(source, slot)).unwrap();
    finalize_activation(state, actor, source, slot)
}

fn finalize_activation(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    slot: u8,
) -> StackItemId {
    let decision = engine::advance_until_decision(state);
    assert!(matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(state.engine.pending_activation.is_none());
    let row = state.stack.last().expect("genuine finalized paid row");
    assert_eq!(row.source, source);
    assert_eq!(row.controller, actor);
    assert_eq!(row.kind, StackItemKind::ActivatedAbility);
    assert_eq!(row.v4.activated_ability_index, Some(slot));
    validate(state);
    row.v4.stack_item_id
}

fn tokens(state: &GameState, actor: PlayerId, name: &str) -> usize {
    let definition = card_id_by_name(name).unwrap();
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).card_def == definition)
        .count()
}

#[test]
fn genuine_paid_season_and_hatcher_double_real_combat_incubation_pay_three_and_restore_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let season = cast(&mut state, actor, "Doubling Season");
        let hatcher = cast(&mut state, actor, "Drake Hatcher");
        assert_eq!(state.objects.get(hatcher).counters.incubation, 0);
        // The real next turn removes summoning sickness. A real Think Twice
        // then produces one printed Prowess trigger, not a supplied boost.
        advance_to_next_main_for(&mut state, actor);
        assert!(!state.objects.get(hatcher).summoning_sick);
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        cast(&mut state, actor, "Think Twice");
        assert_eq!(
            engine::checked_effective_pt(&state, hatcher).unwrap(),
            (2, 4)
        );
        let life = state.players[actor.opponent().index()].life;
        let generation = state.objects.get(hatcher).zone_change_count;
        let mut attacked = false;
        let mut published = false;
        for _ in 0..128 {
            let decision = engine::advance_until_decision(&mut state);
            if attacked
                && state.stack.iter().any(|item| {
                    item.source == hatcher
                        && matches!(
                            item.inline_effect,
                            Some(crate::effect::EffectOp::DrakeIncubationFromCaptureV1 { .. })
                        )
                })
            {
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                published = true;
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { player, .. } => {
                    assert_eq!(player, actor);
                    engine::step(&mut state, Action::DeclareAttackers(vec![hatcher])).unwrap();
                    attacked = true;
                }
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap();
                }
                Decision::OrderTriggers { pending, .. } => {
                    engine::step(
                        &mut state,
                        Action::OrderTriggers((0..pending.len()).collect()),
                    )
                    .unwrap();
                }
                other => panic!("genuine Hatcher combat: {other:?}"),
            }
        }
        assert!(published && attacked);
        assert_eq!(state.players[actor.opponent().index()].life, life - 2);
        assert_eq!(state.objects.get(hatcher).counters.incubation, 0);
        assert!(state.engine.event_history.iter().any(|event| matches!(event,
            CommittedEvent::CombatDamageToPlayer { source, source_zone_change_count, player, amount: 2 }
                if *source == hatcher && *source_zone_change_count == generation && *player == actor.opponent())));
        assert_eq!(state.engine.event_history.iter().filter_map(drake::journal).filter(|record| matches!(record.value,
            JournalV1::Captured { source, player, amount: 2, .. } if source.source == hatcher && player == actor.opponent())).count(), 1);
        assert_eq!(state.objects.get(season).zone, Zone::Battlefield);
        let mut replay = restored(&state);
        let mut countered_outputs = Vec::new();
        for branch in [&mut state, &mut replay] {
            // Restore before the genuine captured combat trigger resolves, so
            // both worlds independently apply Season to the earned amount.
            finish(branch);
            assert_eq!(branch.objects.get(hatcher).counters.incubation, 4);
            let id = paid_activation(branch, actor, hatcher, 0);
            assert_eq!(branch.objects.get(hatcher).counters.incubation, 1);
            assert!(branch.engine.event_history.iter().filter_map(drake::journal).any(|record| matches!(record.value,
                JournalV1::Debit { source, before: 4, after: 1, count: 3, .. } if source.source == hatcher)));
            // Counter the actual paid row in a separate branch. This is the
            // existing engine counter primitive, not an invented counterspell.
            let mut countered = restored(branch);
            assert!(engine::counter_stack_item_by_id(&mut countered, id)
                .unwrap()
                .is_some());
            finish(&mut countered);
            assert_eq!(countered.objects.get(hatcher).counters.incubation, 1);
            assert_eq!(tokens(&countered, actor, "Drake Token"), 0);
            assert!(countered.engine.event_history.iter().filter_map(drake::journal).any(|record| matches!(&record.value,
                JournalV1::Removed { item, kind: RemovalV1::Countered, .. } if item.v4.stack_item_id == id)));
            countered_outputs.push(countered);
            finish(branch);
            assert_eq!(branch.objects.get(hatcher).counters.incubation, 1);
            assert_eq!(tokens(branch, actor, "Drake Token"), 2);
            let facts = crate::doubling_season_public_v48::capture(branch).unwrap();
            let creation = facts
                .creations
                .iter()
                .find(|row| row.token_definition == card_id_by_name("Drake Token").unwrap())
                .unwrap();
            assert_eq!(creation.requested, 1);
            assert_eq!(creation.created.len(), 2);
            assert_eq!(creation.replacement_sources.len(), 1);
            assert_eq!(creation.replacement_sources[0].arena_id, season.0);
        }
        same(&state, &replay);
        same(&countered_outputs[0], &countered_outputs[1]);
    }
}

#[test]
fn genuine_paid_season_and_amulet_sacrifice_draw_two_soul_then_original_loss_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let season = cast(&mut state, actor, "Doubling Season");
        let amulet = cast(&mut state, actor, "Ravenous Amulet");
        let chosen = cast(&mut state, actor, "Llanowar Elves");
        let spare = cast(&mut state, actor, "Llanowar Elves");
        mana(&mut state, actor, &[ManaColor::G]);
        priority(&mut state, actor);
        let hand = state.players[actor.index()].hand.len();
        let library = state.players[actor.index()].library.len();
        let generation = state.objects.get(amulet).zone_change_count;
        engine::step(&mut state, Action::ActivateAbility(amulet, 0)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseCostTargets { ref candidates, remaining: 1, .. }
                if candidates.len() == 2 && candidates.contains(&chosen) && candidates.contains(&spare)));
        let mut replay = restored(&state);
        let mut countered_outputs = Vec::new();
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::ChooseCostTarget(chosen)).unwrap();
            let id = finalize_activation(branch, actor, amulet, 0);
            assert_eq!(pool(branch, actor), 0);
            assert!(branch.objects.get(amulet).tapped);
            assert_eq!(branch.objects.get(chosen).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(spare).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(amulet).counters.soul, 0);
            let payment = branch
                .engine
                .event_history
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::RavenousPaymentV1 { paid }
                        if paid.item.v4.stack_item_id == id =>
                    {
                        Some(paid)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(payment.original_soul_count, None);
            assert_eq!(payment.original_sacrifice.source, chosen);
            assert!(
                payment.tap_history_index < payment.sacrificed_history_index
                    && payment.sacrificed_history_index < payment.sacrifice_history_index
            );
            assert!(
                matches!(&branch.engine.event_history[payment.sacrificed_history_index as usize],
                CommittedEvent::Sacrificed { object, controller_before, .. } if *object == chosen && *controller_before == actor)
            );
            let mut countered = restored(branch);
            assert!(engine::counter_stack_item_by_id(&mut countered, id)
                .unwrap()
                .is_some());
            finish(&mut countered);
            assert_eq!(countered.players[actor.index()].hand.len(), hand);
            assert_eq!(countered.objects.get(amulet).counters.soul, 0);
            assert!(countered.objects.get(amulet).tapped);
            assert_eq!(countered.objects.get(chosen).zone, Zone::Graveyard);
            assert!(countered.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::RavenousCompletedV1 { completed } if completed.operation == OperationV1::Countered)));
            countered_outputs.push(countered);
            finish(branch);
            assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
            assert_eq!(branch.players[actor.index()].library.len(), library - 1);
            assert_eq!(branch.objects.get(amulet).counters.soul, 2);
            let (at, input, source) = branch
                .engine
                .event_history
                .iter()
                .enumerate()
                .find_map(|(at, event)| match event {
                    CommittedEvent::SoulCounterAddedV1 {
                        input_history_index,
                        source,
                        player,
                        replacement,
                        before: 0,
                        count: 2,
                        after: 2,
                    } if source.source == amulet && *player == actor => {
                        assert_eq!(source.zone_change_count, generation);
                        assert_eq!(replacement.controller, actor);
                        assert_eq!(replacement.seasons.len(), 1);
                        assert_eq!(replacement.seasons[0].source, season);
                        Some((at, *input_history_index, *source))
                    }
                    _ => None,
                })
                .expect("actual doubled soul occurrence");
            assert_eq!(input as usize + 1, at);
            assert!(matches!(&branch.engine.event_history[input as usize],
                CommittedEvent::SoulCounterInputV1 { input } if input.actor == actor && input.source == source && input.world.objects.get(amulet).counters.soul == 0));
            assert_eq!(
                branch
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(event, CommittedEvent::SoulCounterAddedV1 { .. }))
                    .count(),
                1
            );
        }
        same(&state, &replay);
        same(&countered_outputs[0], &countered_outputs[1]);
        advance_to_next_main_for(&mut state, actor);
        assert!(!state.objects.get(amulet).tapped);
        mana(&mut state, actor, &[ManaColor::G; 4]);
        let life = state.players[actor.opponent().index()].life;
        let id = paid_activation(&mut state, actor, amulet, 1);
        assert_eq!(pool(&state, actor), 0);
        assert_eq!(state.objects.get(amulet).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(amulet).counters.soul, 0);
        assert!(state.engine.event_history.iter().any(|event| matches!(event,
            CommittedEvent::RavenousPaymentV1 { paid } if paid.item.v4.stack_item_id == id && paid.original_soul_count == Some(2) && paid.original_source.zone_change_count == generation)));
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            finish(branch);
            assert_eq!(branch.players[actor.opponent().index()].life, life - 2);
            assert!(branch
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
                CommittedEvent::LifeLoss { player, amount: 2 } if *player == actor.opponent())));
            assert!(branch.engine.event_history.iter().any(|event| matches!(event,
                CommittedEvent::RavenousRemovedV1 { removed } if removed.item.v4.stack_item_id == id && removed.kind == RemovalKindV1::Resolving)));
            assert_eq!(
                branch
                    .engine
                    .event_history
                    .iter()
                    .filter(|event| matches!(event, CommittedEvent::SoulCounterAddedV1 { .. }))
                    .count(),
                1
            );
            assert_eq!(branch.objects.get(season).zone, Zone::Battlefield);
        }
        same(&state, &replay);
    }
}
