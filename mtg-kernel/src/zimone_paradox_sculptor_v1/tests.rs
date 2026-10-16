use super::*;
use crate::{
    card_def::{self, CardCapability, TargetSpec},
    engine::{self, Action, Decision},
    event::{self, ProposedEvent},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    standard_cards_v1::StandardTargetV1,
    standard_creatures_v1::{CounterExtrasV1, CounterTransferV1},
    state::{Counters, GameState, Step, Target},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for _ in 0..10 {
        put(&mut state, actor, "Forest", Zone::Battlefield);
        put(&mut state, actor, "Island", Zone::Battlefield);
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
        Decision::ChooseTargets {
            can_finish: true, ..
        } => Action::FinishTargetSelection,
        other => panic!("native progress: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        quiet(state, decision);
    }
    panic!("actor priority missing");
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        quiet(state, decision);
    }
    panic!("paid native resolution did not finish");
}

fn tapped_lands(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let object = state.objects.get(**id);
            object.tapped && card_def::CARD_DEFS[usize::from(object.card_def)].is_land
        })
        .count()
}

fn paid(state: &mut GameState, actor: PlayerId, name: &str) -> ObjectId {
    priority(state, actor);
    let source = put(state, actor, name, Zone::Hand);
    let value =
        card_def::CARD_DEFS[usize::from(state.objects.get(source).card_def)].mana_value as usize;
    let before = tapped_lands(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    assert_eq!(tapped_lands(state, actor) - before, value);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    source
}

fn next_own_main(state: &mut GameState, actor: PlayerId) {
    let turn = state.turn;
    for _ in 0..512 {
        let decision = engine::advance_until_decision(state);
        if state.turn > turn
            && state.active_player == actor
            && state.step == Step::Main1
            && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            return;
        }
        quiet(state, decision);
    }
    panic!("next genuine own main phase missing");
}

fn paid_ready(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready(actor);
    let source = paid(&mut state, actor, "Zimone, Paradox Sculptor");
    priority(&mut state, actor);
    let before = state.clone();
    assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
    assert_eq!(
        state, before,
        "summoning sickness must refuse before payment"
    );
    next_own_main(&mut state, actor);
    assert!(!state.objects.get(source).summoning_sick);
    (state, source)
}

fn select_trigger(state: &mut GameState, actor: PlayerId, source: ObjectId, targets: &[ObjectId]) {
    for (index, target) in targets.iter().enumerate() {
        let decision = engine::advance_until_decision(state);
        assert!(
            matches!(decision, Decision::ChooseTargets { player, spell, ref legal_targets, can_finish: true, .. }
            if player == actor && spell == source && legal_targets.contains(&Target::Object(*target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(*target))).unwrap();
        if index == 0 && targets.len() == 2 {
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::ChooseTargets {
                    can_finish: true,
                    ..
                }
            ));
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            assert_eq!(&restored, state);
            *state = restored;
        }
    }
    if targets.len() < 2 {
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(state, Action::FinishTargetSelection).unwrap();
    }
}

fn select_activation(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    targets: &[ObjectId],
) {
    for (index, target) in targets.iter().enumerate() {
        assert!(matches!(engine::advance_until_decision(state),
            Decision::ChooseEffectTargets { player, source: original, selected_count,
                ref legal_targets, can_finish: true, .. }
            if player == actor && original == source && usize::from(selected_count) == index
                && legal_targets.contains(&Target::Object(*target))));
        engine::step(state, Action::ChooseEffectTarget(Target::Object(*target))).unwrap();
        if index == 0 && targets.len() == 2 {
            assert!(matches!(
                engine::advance_until_decision(state),
                Decision::ChooseEffectTargets {
                    selected_count: 1,
                    can_finish: true,
                    ..
                }
            ));
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
            assert_eq!(&restored, state);
            *state = restored;
        }
    }
    if targets.len() < 2 {
        assert!(matches!(
            engine::advance_until_decision(state),
            Decision::ChooseEffectTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
}

fn assert_activation_published(
    state: &mut GameState,
    actor: PlayerId,
    source: ObjectId,
    targets: &[ObjectId],
) {
    // The final optional pick only appends pending targets. Genuine Advance
    // finalizes payment and publishes the ability before any response scene.
    assert!(matches!(engine::advance_until_decision(state),
        Decision::CastSpellOrPass { player, .. } if player == actor));
    assert!(state.engine.pending_activation.is_none());
    let item = state.stack.last().unwrap();
    assert_eq!(item.source, source);
    assert_eq!(
        item.targets,
        targets
            .iter()
            .copied()
            .map(Target::Object)
            .collect::<Vec<_>>()
    );
    assert_eq!(item.controller, actor);
    assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
}

fn activate(state: &mut GameState, actor: PlayerId, source: ObjectId, targets: &[ObjectId]) {
    priority(state, actor);
    let before = tapped_lands(state, actor);
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    select_activation(state, actor, source, targets);
    assert_activation_published(state, actor, source, targets);
    assert!(state.objects.get(source).tapped);
    assert_eq!(tapped_lands(state, actor) - before, 2);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    let item = state.stack.last().unwrap();
    assert_eq!(
        item.targets,
        targets
            .iter()
            .copied()
            .map(Target::Object)
            .collect::<Vec<_>>()
    );
    assert_eq!(item.controller, actor);
    assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
}

fn current(state: &GameState, target: ObjectId) -> CounterTransferV1 {
    let live = state.objects.get(target);
    CounterTransferV1 {
        counters: live.counters,
        extras: CounterExtrasV1 {
            loyalty: crate::planeswalker_v1::loyalty(state, target).unwrap_or(0),
            lifelink: live.v4.lifelink_keyword_counters,
            time: live.v4.time_counters_v1,
            finality: live.v4.creature_upgrade.as_ref().map_or(0, |v| v.finality),
        },
    }
}

#[test]
fn paid_printed_costs_zero_one_two_targets_complete_native_restore_and_current_counts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (base, source) = paid_ready(actor);
        let definition = &card_def::CARD_DEFS[usize::from(base.objects.get(source).card_def)];
        assert_eq!(definition.capability, CardCapability::Partial);
        assert_eq!(
            definition.activated_abilities[0].target_spec,
            TargetSpec::StandardV1(StandardTargetV1::UpToTwoControlledArtifactsOrCreatures)
        );
        for count in 0..=2 {
            let mut state = base.clone();
            // Genuine existing artifacts are primitive initial support, not
            // claimed paid casts. Counter setup is the typed native placement.
            let a = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            let b = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
            for target in [a, b] {
                crate::standard_creatures_v1::place_counters_checked_v1(
                    &mut state,
                    target,
                    actor,
                    CounterTransferV1 {
                        counters: Counters {
                            charge: 2,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            activate(&mut state, actor, source, &[a, b][..count]);
            // A native response-scene placement changes CURRENT resolution
            // counts. It is not announcement LKI or a new printed ability.
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state,
                a,
                actor,
                CounterTransferV1 {
                    counters: Counters {
                        charge: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            let serialized = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&serialized).unwrap();
            settle(&mut state);
            settle(&mut restored);
            assert_eq!(state, restored);
            assert_eq!(state.state_hash(), restored.state_hash());
            assert_eq!(
                state.objects.get(a).counters.charge,
                if count > 0 { 6 } else { 3 }
            );
            assert_eq!(
                state.objects.get(b).counters.charge,
                if count > 1 { 4 } else { 2 }
            );
            assert_eq!(state.objects.get(a).counters.plus1_plus1, 0);
            assert!(state.engine.event_history.iter().all(|e| !matches!(
                e,
                event::CommittedEvent::PlusOneCountersAdded { count: 0, .. }
            )));
        }
    }
}

#[test]
fn paid_beginning_combat_zero_one_two_controlled_creatures_and_pending_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in 0..=2 {
            let mut state = ready(actor);
            let source = paid(&mut state, actor, "Zimone, Paradox Sculptor");
            let a = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let b = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let artifact = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            let enemy = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            for _ in 0..64 {
                let decision = engine::advance_until_decision(&mut state);
                if let Decision::ChooseTargets {
                    spell,
                    player,
                    ref legal_targets,
                    can_finish,
                    ..
                } = decision
                {
                    assert_eq!((spell, player, can_finish), (source, actor, true));
                    assert!(!legal_targets.contains(&Target::Object(artifact)));
                    assert!(!legal_targets.contains(&Target::Object(enemy)));
                    break;
                }
                quiet(&mut state, decision);
            }
            assert_eq!(state.step, Step::BeginCombat);
            select_trigger(&mut state, actor, source, &[a, b][..count]);
            let saved = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&saved).unwrap();
            settle(&mut state);
            settle(&mut restored);
            assert_eq!(state, restored);
            assert_eq!(
                state.objects.get(a).counters.plus1_plus1,
                i32::from(count > 0)
            );
            assert_eq!(
                state.objects.get(b).counters.plus1_plus1,
                i32::from(count > 1)
            );
            assert_eq!(state.objects.get(enemy).counters.plus1_plus1, 0);
            assert_eq!(state.objects.get(artifact).counters.plus1_plus1, 0);
        }
    }
}

#[test]
fn paid_all_counter_kinds_add_current_counts_through_season_and_keep_unsigned_loyalty_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let (mut state, source) = paid_ready(actor);
            let artifact = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            // Explicit native counter-placement support. The artifact keeps
            // opposite-sign kinds separate from the source's +1/+1 kind so
            // ordinary state-based cancellation cannot obscure the operands.
            let initial = CounterTransferV1 {
                counters: Counters {
                    minus1_minus1: 1,
                    minus0_minus1: 1,
                    stun: 1,
                    lore: 1,
                    oil: 1,
                    charge: 1,
                    net: 1,
                    bait: 1,
                    soul: 1,
                    incubation: 1,
                    revival: 1,
                    fellowship: 1,
                    ..Default::default()
                },
                extras: CounterExtrasV1 {
                    loyalty: 1_100_000_000,
                    lifelink: 1,
                    time: 1,
                    finality: 1,
                },
            };
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state, artifact, actor, initial,
            )
            .unwrap();
            event::add_plus_one_counters(&mut state, source, actor, 1).unwrap();
            if season {
                paid(&mut state, actor, "Doubling Season");
            }
            activate(&mut state, actor, source, &[artifact, source]);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            settle(&mut state);
            settle(&mut restored);
            assert_eq!(state, restored);
            let multiplier = if season { 3 } else { 2 };
            assert_eq!(
                current(&state, artifact),
                CounterTransferV1 {
                    counters: Counters {
                        minus1_minus1: multiplier as i16,
                        minus0_minus1: multiplier as i16,
                        stun: multiplier as i16,
                        lore: multiplier as i16,
                        oil: multiplier as i16,
                        charge: multiplier,
                        net: multiplier,
                        bait: multiplier,
                        soul: multiplier,
                        incubation: multiplier,
                        revival: multiplier,
                        fellowship: multiplier,
                        ..Default::default()
                    },
                    extras: CounterExtrasV1 {
                        loyalty: 1_100_000_000 * multiplier as u32,
                        lifelink: multiplier as i16,
                        time: multiplier as u8,
                        finality: multiplier as u16,
                    },
                }
            );
            assert_eq!(state.objects.get(source).counters.plus1_plus1, multiplier);
            crate::doubling_season_v48::validate_token_journals(&state).unwrap();
            assert!(state.engine.halted.is_none());
        }
    }
}

#[test]
fn paid_two_target_overflow_keeps_both_counter_ledgers_unchanged_after_cost_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (base, source) = paid_ready(actor);
        for kind in 0..7 {
            let mut state = base.clone();
            let a = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
            let b = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
            // Exact native-width counter operands, not claimed naturally
            // accumulated counters or an invented printed placement ability.
            let mut original = CounterTransferV1::default();
            match kind {
                0 => original.counters.plus1_plus1 = i32::MAX,
                1 => original.counters.charge = i32::MAX,
                2 => original.counters.oil = i16::MAX,
                3 => original.extras.time = u8::MAX,
                4 => original.extras.finality = u16::MAX,
                5 => original.extras.loyalty = u32::MAX,
                _ => original.counters.fellowship = i32::MAX,
            }
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state,
                a,
                actor,
                CounterTransferV1 {
                    counters: Counters {
                        charge: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            crate::standard_creatures_v1::place_counters_checked_v1(&mut state, b, actor, original)
                .unwrap();
            activate(&mut state, actor, source, &[a, b]);
            let before = [current(&state, a), current(&state, b)];
            for _ in 0..8 {
                let decision = engine::advance_until_decision(&mut state);
                if state.engine.halted.is_some() {
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                let result = engine::step(&mut state, Action::Pass);
                assert!(
                    result.is_ok() || state.engine.halted.is_some(),
                    "{result:?}"
                );
            }
            assert_eq!(
                state.engine.halted,
                Some((
                    engine::UnsupportedMechanic::InvalidEffectContinuation,
                    source
                ))
            );
            assert_eq!([current(&state, a), current(&state, b)], before);
            assert!(state.objects.get(source).tapped);
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn paid_target_rejection_original_source_departure_and_simultaneous_control_reentry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid_ready(actor);
        let a = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let b = put(&mut state, actor, "Fishing Pole", Zone::Battlefield);
        let enemy = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        for target in [a, b] {
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state,
                target,
                actor,
                CounterTransferV1 {
                    counters: Counters {
                        charge: 2,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        priority(&mut state, actor);
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseEffectTargets { source: original, selected_count: 0, .. }
                if original == source));
        let before = state.clone();
        assert!(engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(enemy))
        )
        .is_err());
        assert_eq!(state, before);
        engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(a))).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::ChooseEffectTargets { ref legal_targets, .. }
            if !legal_targets.contains(&Target::Object(a)))
        );
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(a))).is_err());
        assert_eq!(state, before);
        engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(b))).unwrap();
        assert_activation_published(&mut state, actor, source, &[a, b]);
        let paid = state.clone();
        assert!(paid.objects.get(source).tapped);
        assert_eq!(paid.players[actor.index()].mana_pool, [0; 6]);
        for reenter_source in [false, true] {
            let mut branch = paid.clone();
            // Explicit native zone-change primitives during a response window.
            // The original activated item is never rewritten or re-announced.
            event::propose_and_commit(
                &mut branch,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            if reenter_source {
                event::propose_and_commit(
                    &mut branch,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            }
            settle(&mut branch);
            assert_eq!(branch.objects.get(a).counters.charge, 4);
            assert_eq!(branch.objects.get(b).counters.charge, 4);
        }
        // A source-control change cannot change the captured ability
        // controller. This is a native control primitive, not a paid spell.
        let mut control_branch = paid.clone();
        control_branch.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        control_branch.players[actor.opponent().index()]
            .battlefield
            .push(source);
        control_branch.objects.get_mut(source).controller = actor.opponent();
        control_branch.objects.get_mut(source).summoning_sick = true;
        settle(&mut control_branch);
        assert_eq!(control_branch.objects.get(a).counters.charge, 4);
        assert_eq!(control_branch.objects.get(b).counters.charge, 4);
        // Explicit current-control primitive, not a claimed paid control spell.
        // Maintain the native rosters while independently blinking the other
        // original target, so both slots become illegal for distinct reasons.
        let mut branch = paid.clone();
        branch.players[actor.index()]
            .battlefield
            .retain(|id| *id != a);
        branch.players[actor.opponent().index()].battlefield.push(a);
        branch.objects.get_mut(a).controller = actor.opponent();
        branch.objects.get_mut(a).summoning_sick = true;
        event::propose_and_commit(&mut branch, ProposedEvent::zone_change(b, Zone::Exile));
        event::propose_and_commit(
            &mut branch,
            ProposedEvent::zone_change(b, Zone::Battlefield),
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&branch).unwrap()).unwrap();
        settle(&mut branch);
        settle(&mut restored);
        assert_eq!(branch, restored);
        assert_eq!(branch.objects.get(a).counters.charge, 2);
        assert_eq!(branch.objects.get(b).counters.charge, 0);
    }
}

#[test]
fn paid_current_effective_type_loss_skips_original_slot_without_reassigning_second_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid_ready(actor);
        let face = put(&mut state, actor, "Doubling Season", Zone::Battlefield);
        // Explicit native face-down/face-up characteristic primitives, not a
        // claim that this printing supplies a paid turn-face-up ability.
        state.objects.get_mut(face).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 1 << actor.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let artifact = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        for target in [face, artifact] {
            crate::standard_creatures_v1::place_counters_checked_v1(
                &mut state,
                target,
                actor,
                CounterTransferV1 {
                    counters: Counters {
                        charge: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        }
        activate(&mut state, actor, source, &[face, artifact]);
        let incarnation = state.objects.get(face).zone_change_count;
        state.objects.get_mut(face).v4.face_down_v1 = None;
        assert_eq!(state.objects.get(face).zone_change_count, incarnation);
        assert!(!engine::object_has_type(
            &state,
            face,
            card_def::CardType::Creature
        ));
        assert!(!engine::object_has_type(
            &state,
            face,
            card_def::CardType::Artifact
        ));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(state, restored);
        assert_eq!(state.objects.get(face).counters.charge, 1);
        // The second original slot remains legal. Its added one is replaced
        // by the now-face-up Season, so the current total becomes three.
        assert_eq!(state.objects.get(artifact).counters.charge, 3);
    }
}
