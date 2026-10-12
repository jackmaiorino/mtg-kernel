//! Requires genuine future registration. Physical characteristic boundaries
//! below are labeled and do not claim a printed FDN modifier producer.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::ids::PlayerId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state};

pub(super) fn ready(actor: PlayerId) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    state.step = crate::state::Step::Main1;
    // Real printed draw attempts need real cards. ready_state intentionally
    // starts with empty libraries; it never deals an opening hand.
    for owner in [actor, actor.opponent()] {
        for _ in 0..16 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    state
}
pub(super) fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
pub(super) fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected genuine draw stage {other:?}"),
        }
    }
    panic!("draw resolution did not settle");
}
pub(super) fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    green: u8,
    generic: u8,
) -> ObjectId {
    let source = put(state, actor, name, Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = green;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = generic;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}
pub(super) fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn readonly(state: &GameState) {
    for _ in 0..2 {
        let mut bad = restore(state);
        let before = serde_json::to_vec(&bad).unwrap();
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), before);
    }
}

#[test]
fn actual_cast_conditional_etb_and_later_event_time_draw_trample_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let hand = state.players[actor.index()].hand.len();
        let source = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
        assert_eq!(state.players[actor.index()].hand.len(), hand);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(elf, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(state.players[actor.index()].hand.len(), hand);
        assert!(engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        let before = state.players[actor.index()].hand.len();
        cast(&mut state, actor, "Myr Enforcer", 0, 7);
        assert_eq!(state.players[actor.index()].hand.len(), before + 1);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        assert!(!engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        let other = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(other, Zone::Battlefield),
        );
        assert!(engine::has_effective_keyword(
            &state,
            other,
            Keywords::TRAMPLE
        ));
        let before = state.players[actor.index()].hand.len();
        let large = put(&mut state, actor, "Myr Enforcer", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(large, Zone::Battlefield),
        );
        settle(&mut state);
        assert_eq!(
            state.players[actor.index()].hand.len(),
            before,
            "a creature entering for the source's opponent creates no draw"
        );
    }
}

#[test]
fn own_etb_rechecks_ferocious_but_entrant_capture_survives_departure_and_control() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let large = cast(&mut state, actor, "Myr Enforcer", 0, 7);
        let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 3;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..20 {
            next(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        let before = state.players[actor.index()].hand.len();
        crate::event::propose_and_commit(&mut state, ProposedEvent::zone_change(large, Zone::Hand));
        settle(&mut state);
        assert_eq!(
            state.players[actor.index()].hand.len(),
            before + 1,
            "returned creature only, no conditional draw"
        );
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(large, Zone::Battlefield),
        );
        next(&mut state);
        let before = state.players[actor.index()].hand.len();
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(large, Zone::Graveyard),
        );
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.players[actor.index()].hand.len(), before + 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn simultaneous_anthems_are_all_applied_before_every_entrant_power_observation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reversed in [false, true] {
            let mut state = ready(actor);
            cast(&mut state, actor, "Garruk's Uprising", 1, 2);
            let mut birds = (0..3)
                .map(|_| put(&mut state, actor, "Empyrean Eagle", Zone::Hand))
                .collect::<Vec<_>>();
            if reversed {
                birds.reverse();
            }
            crate::event::propose_and_commit_batch(
                &mut state,
                birds
                    .iter()
                    .map(|id| ProposedEvent::zone_change(*id, Zone::Battlefield))
                    .collect(),
            );
            assert!(birds
                .iter()
                .all(|id| engine::effective_power(&state, *id) == 4));
            let before = state.players[actor.index()].hand.len();
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.players[actor.index()].hand.len(), before + 3);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}

#[test]
fn genuine_pending_and_published_draw_ownership_cannot_be_erased_or_replayed() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor, "Myr Enforcer", 0, 7);
        let source = put(&mut state, actor, "Garruk's Uprising", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 3;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        for _ in 0..20 {
            next(&mut state);
            if state.stack.iter().any(|row| {
                matches!(
                    row.inline_effect,
                    Some(EffectOp::GarrukDrawFromCaptureV1 { .. })
                )
            }) {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        let saved = state
            .stack
            .iter()
            .find(|row| {
                matches!(
                    row.inline_effect,
                    Some(EffectOp::GarrukDrawFromCaptureV1 { .. })
                )
            })
            .unwrap()
            .clone();
        let mut bad = restore(&state);
        bad.stack
            .retain(|row| row.v4.stack_item_id != saved.v4.stack_item_id);
        readonly(&bad);
        let mut bad = restore(&state);
        bad.stack.last_mut().unwrap().controller = actor.opponent();
        readonly(&bad);
        settle(&mut state);
        let mut bad = restore(&state);
        bad.stack.push(saved);
        readonly(&bad);
    }
}

#[test]
fn genuine_copied_enchantment_entry_and_exact_id_counter_keep_original_owners() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        cast(&mut state, actor, "Myr Enforcer", 0, 7);
        let source = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
        let before = state.players[actor.index()].hand.len();
        let copied =
            crate::standard_cards_v1::create_token_copy(&mut state, source, actor).unwrap();
        assert!(state.objects.get(copied).v4.is_token);
        let mut replay = restore(&state);
        settle(&mut state);
        settle(&mut replay);
        assert_eq!(state.players[actor.index()].hand.len(), before + 1);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        let creature = put(&mut state, actor, "Myr Enforcer", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Battlefield),
        );
        loop {
            match next(&mut state) {
                Decision::OrderTriggers { pending, .. } => engine::step(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                )
                .unwrap(),
                Decision::CastSpellOrPass { .. } => break,
                other => panic!("real copied observer placement {other:?}"),
            }
        }
        let saved = state.stack.last().unwrap().clone();
        engine::counter_stack_item_by_id(&mut state, saved.v4.stack_item_id).unwrap();
        validate_state(&state).unwrap();
        let mut bad = restore(&state);
        bad.stack.push(saved);
        readonly(&bad);
        let before = state.players[actor.index()].hand.len();
        settle(&mut state);
        assert_eq!(
            state.players[actor.index()].hand.len(),
            before + 1,
            "countered one observer; the other genuinely draws once"
        );
    }
}

#[test]
fn actual_witness_suppression_and_later_reentered_grant_follow_layer_timestamps() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(elf, Zone::Battlefield),
        );
        settle(&mut state);
        assert!(engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(witness)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&crate::state::Target::Object(elf)))
        );
        engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Object(elf)),
        )
        .unwrap();
        settle(&mut state);
        assert!(!engine::has_effective_keyword(
            &state,
            elf,
            Keywords::TRAMPLE
        ));
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Hand),
        );
        crate::event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        settle(&mut state);
        assert!(
            engine::has_effective_keyword(&state, elf, Keywords::TRAMPLE),
            "later continuously granted keyword survives the earlier ability removal"
        );
        let replay = restore(&state);
        assert!(engine::has_effective_keyword(
            &replay,
            elf,
            Keywords::TRAMPLE
        ));
    }
}

#[test]
fn accepted_nyxborn_x_and_bestow_entry_characteristics_precede_actual_draw_capture() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for form in 0..3 {
            let mut state = ready(actor);
            cast(&mut state, actor, "Garruk's Uprising", 1, 2);
            let host = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(host, Zone::Battlefield),
            );
            settle(&mut state);
            let hydra = put(&mut state, actor, "Nyxborn Hydra", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 2;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            next(&mut state);
            engine::step(&mut state, Action::CastSpell(hydra)).unwrap();
            assert!(matches!(
                next(&mut state),
                Decision::ChooseSpellMode { mode_count: 2, .. }
            ));
            engine::step(&mut state, Action::ChooseSpellMode(u8::from(form != 0))).unwrap();
            if form != 0 {
                assert!(
                    matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                    if legal_targets.contains(&crate::state::Target::Object(host)))
                );
                engine::step(
                    &mut state,
                    Action::ChooseTarget(crate::state::Target::Object(host)),
                )
                .unwrap();
            }
            assert!(
                matches!(next(&mut state), Decision::ChooseEffectOption { option_count, .. } if option_count > 4)
            );
            engine::step(&mut state, Action::ChooseEffectOption(4)).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.last().unwrap().v4.x_value, 4);
            if form == 2 {
                crate::event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(host, Zone::Hand),
                );
            }
            let before = state.players[actor.index()].hand.len();
            let start = state.engine.event_history.len();
            let mut replay = restore(&state);
            settle(&mut state);
            settle(&mut replay);
            assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 4);
            let generation = state.objects.get(hydra).zone_change_count;
            let placements = state.engine.event_history[start..]
                .iter()
                .filter(|event| {
                    matches!(event,
                    CommittedEvent::PlusOneCountersAdded {object,zone_change_count,player,count:4}
                    if *object==hydra && *zone_change_count==generation && *player==actor)
                })
                .count();
            assert_eq!(
                placements, 1,
                "the shared entry logger retains one genuine X-counter occurrence"
            );
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(
                state.players[actor.index()].hand.len(),
                before + usize::from(form != 1)
            );
            let observation = state.engine.event_history[start..]
                .iter()
                .find_map(|event| match event {
                    CommittedEvent::GarrukEntryObservationV1 {
                        entry,
                        effective_creature,
                        effective_power,
                        ..
                    } if entry.object == hydra => {
                        Some((entry, *effective_creature, *effective_power))
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(observation.0.snapshot.counters.plus1_plus1, 4);
            if form == 1 {
                assert!(!observation.1);
                assert_eq!(observation.2, None);
                assert_eq!(observation.0.snapshot.v4.attached_to.unwrap().object, host);
                assert_eq!(engine::effective_power(&state, host), 5);
            } else {
                assert!(observation.1);
                assert_eq!(observation.2, Some(4));
                assert!(observation.0.snapshot.v4.attached_to.is_none());
            }
        }
    }
}

#[test]
fn actual_simultaneous_dwynen_entries_retain_mandatory_draws_through_legend_choice() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
        let entries = [0, 1].map(|_| put(&mut state, actor, "Dwynen, Gilt-Leaf Daen", Zone::Hand));
        crate::event::propose_and_commit_batch(
            &mut state,
            entries
                .iter()
                .map(|id| ProposedEvent::zone_change(*id, Zone::Battlefield))
                .collect(),
        );
        assert!(entries
            .iter()
            .all(|id| engine::effective_power(&state, *id) == 4));
        assert!(matches!(
            next(&mut state),
            Decision::ChooseLegendPermanent { .. }
        ));
        assert_eq!(
            state
                .pending_legend_rule_v1
                .as_ref()
                .unwrap()
                .waiting_triggers()
                .len(),
            2
        );
        validate_state(&state).unwrap();
        let public = crate::garruk_public_v1::observe_v1(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        assert!(public.entry_occurrences.iter().all(|row| row.lifecycle
            == crate::garruk_public_v1::EntryLifecycleV1::LegendWaiting
            && row.event_time_power == Some(4)));
        let mut bad = restore(&state);
        crate::standard_cards_v1::gain_control(&mut bad, source, actor.opponent());
        let mut value = serde_json::to_value(&bad).unwrap();
        let waiting = value["pending_legend_rule_v1"]["waiting_triggers"]
            .as_array_mut()
            .unwrap();
        let rewritten = serde_json::to_value(actor.opponent()).unwrap();
        waiting[0]["controller"] = rewritten.clone();
        waiting[0]["source_contract"]["controller"] = rewritten;
        let bad: GameState = serde_json::from_value(value).unwrap();
        assert!(validate_state(&bad).is_err());
        let bytes = serde_json::to_vec(&bad).unwrap();
        for _ in 0..2 {
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert!(
                engine::step(&mut restored, Action::ChooseLegendPermanent(entries[0])).is_err()
            );
            assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        }
        let before = state.players[actor.index()].hand.len();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseLegendPermanent(entries[0])).unwrap();
            settle(current);
            assert_eq!(current.players[actor.index()].hand.len(), before + 2);
            assert_eq!(current.objects.get(entries[0]).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(entries[1]).zone, Zone::Graveyard);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
