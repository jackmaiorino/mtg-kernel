//! Definition-owned Fiery Annihilation resolution and exact target relations.
use crate::card_def::{Subtype, TargetSpec};
use crate::effect::ExecCtx;
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{
    AbilitySourceContractV4, GameState, ObjectLinkV4, StackTargetContractV4, Target, Zone,
};

pub(crate) fn original_battlefield_target(
    state: &GameState,
    target: Target,
    contract: StackTargetContractV4,
) -> bool {
    let StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        zone: Zone::Battlefield,
        zone_change_count,
        spell_copy_origin,
        ..
    } = contract
    else {
        return false;
    };
    target == Target::Object(object)
        && state.objects.try_get(object).is_some_and(|live| {
            live.zone == Zone::Battlefield
                && live.card_def == card_def
                && live.owner == owner
                && live.zone_change_count == zone_change_count
                && live.spell_copy_origin == spell_copy_origin
        })
        && state
            .players
            .iter()
            .any(|player| player.battlefield.contains(&object))
}

pub(crate) fn attached_equipment(
    state: &GameState,
    equipment: ObjectId,
    creature: ObjectId,
    creature_generation: u32,
) -> bool {
    state.objects.try_get(equipment).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && crate::engine::has_effective_subtype(state, equipment, Subtype::Equipment)
            && live.v4.attached_to
                == Some(ObjectLinkV4 {
                    object: creature,
                    zone_change_count: creature_generation,
                })
    }) && state.objects.try_get(creature).is_some_and(|host| {
        host.zone == Zone::Battlefield
            && host.zone_change_count == creature_generation
            && host.attachments.contains(&equipment)
    })
}

pub(crate) fn target_legalities(
    state: &GameState,
    source: ObjectId,
    controller: PlayerId,
    targets: &[Target],
    contracts: &[StackTargetContractV4],
) -> [bool; 2] {
    if state.objects.try_get(source).is_none()
        || !(1..=2).contains(&targets.len())
        || contracts.len() != targets.len()
        || !targets
            .iter()
            .all(|target| matches!(target, Target::Object(_)))
        || !contracts.iter().all(|contract| {
            matches!(
                contract,
                StackTargetContractV4::Object {
                    zone: Zone::Battlefield,
                    ..
                }
            )
        })
        || targets.len() == 2 && targets[0] == targets[1]
    {
        return [false; 2];
    }
    let original_creature = original_battlefield_target(state, targets[0], contracts[0]);
    let creature_legal = original_creature
        && crate::engine::effect_target_is_legal(
            state,
            source,
            controller,
            TargetSpec::Creature,
            targets,
            0,
        );
    let equipment_legal = if targets.len() == 2
        && original_creature
        && original_battlefield_target(state, targets[1], contracts[1])
    {
        let Target::Object(creature) = targets[0] else {
            return [false; 2];
        };
        let Target::Object(equipment) = targets[1] else {
            return [false; 2];
        };
        attached_equipment(
            state,
            equipment,
            creature,
            state.objects.get(creature).zone_change_count,
        ) && crate::engine::effect_target_is_legal(
            state,
            source,
            controller,
            TargetSpec::AnyPermanent,
            targets,
            1,
        )
    } else {
        false
    };
    [creature_legal, equipment_legal]
}

pub(crate) fn execute(ctx: &ExecCtx, state: &mut GameState) {
    let [creature_legal, equipment_legal] = target_legalities(
        state,
        ctx.source,
        ctx.controller,
        &ctx.targets,
        &ctx.target_contracts,
    );
    if !creature_legal && !equipment_legal {
        return;
    }
    // Preflight the replacement identity before damage or Equipment exile can
    // mutate state, then capture source facts while its original spell is live.
    if creature_legal && state.engine.next_replacement_id.checked_add(1).is_none() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            ctx.source,
        ));
        return;
    }
    let mut source = AbilitySourceContractV4::capture(state, ctx.source);
    source.controller = ctx.controller;
    if creature_legal {
        event::propose_and_commit(state, ProposedEvent::damage(ctx.source, ctx.targets[0], 5));
    }
    if equipment_legal {
        let Target::Object(equipment) = ctx.targets[1] else {
            return;
        };
        event::propose_and_commit(state, ProposedEvent::zone_change(equipment, Zone::Exile));
    }
    // SBA waits until the whole spell completes: the shield is installed even
    // when all five damage was prevented, before lethal damage is processed.
    if creature_legal {
        event::install_exile_if_dies(state, source, ctx.target_contracts[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::Keywords;
    use crate::engine::{self, Action, Decision, EffectDuration, Layers, UntilEndOfTurnEffect};
    use crate::policy_observation_v6::tests::{put, ready_state};

    fn fixture(
        caster: PlayerId,
        optional: bool,
        survives: bool,
    ) -> (GameState, ObjectId, ObjectId, ExecCtx) {
        let mut state = ready_state();
        state.active_player = caster;
        state.priority_player = caster;
        let source = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        let creature = put(
            &mut state,
            caster.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        if survives {
            state.objects.get_mut(creature).counters.plus1_plus1 = 6;
        }
        let equipment = put(&mut state, caster, "Viridian Longbow", Zone::Battlefield);
        state
            .attach_object_exact(equipment, 0, creature, 0)
            .unwrap();
        let mut ctx = ExecCtx::no_targets(source, caster);
        ctx.targets = vec![Target::Object(creature)];
        if optional {
            ctx.targets.push(Target::Object(equipment));
        }
        ctx.target_contracts = ctx
            .targets
            .iter()
            .map(|&target| StackTargetContractV4::capture(&state, target))
            .collect();
        (state, creature, equipment, ctx)
    }

    fn grant(state: &mut GameState, object: ObjectId, keywords: Keywords) {
        let generation = state.objects.get(object).zone_change_count;
        state
            .engine
            .until_end_of_turn
            .push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                object_id: object,
                object_zone_change_count: generation,
                layer: Layers::ABILITY_ADDING,
                timestamp: 999,
                duration: EffectDuration::EndOfTurn,
                keywords,
            });
    }

    #[test]
    fn fiery_primitive_five_damage_optional_equipment_exile_and_lethal_replacement_restore() {
        for caster in [PlayerId::P0, PlayerId::P1] {
            for optional in [false, true] {
                let (mut state, creature, equipment, ctx) = fixture(caster, optional, false);
                // A second Equipment attached after announcement is not a
                // selected target and remains on the battlefield.
                let late_equipment = put(&mut state, caster, "Viridian Longbow", Zone::Battlefield);
                state
                    .attach_object_exact(late_equipment, 0, creature, 0)
                    .unwrap();
                let before = state.engine.event_history.len();
                let mut restored: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for current in [&mut state, &mut restored] {
                    execute(&ctx, current);
                    assert_eq!(current.objects.get(creature).damage, 5);
                    assert_eq!(
                        current.objects.get(equipment).zone,
                        if optional {
                            Zone::Exile
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert!(
                        matches!(current.engine.event_history[before], event::CommittedEvent::Damage { source, target: Target::Object(target), amount: 5 } if source == ctx.source && target == creature)
                    );
                    event::validate_exile_if_dies_replacements(current).unwrap();
                    crate::trigger::sba_fixed_point(current);
                    assert_eq!(current.objects.get(creature).zone, Zone::Exile);
                    assert!(current.players[caster.opponent().index()]
                        .graveyard
                        .is_empty());
                    assert!(current.objects.get(equipment).v4.attached_to.is_none());
                    assert_eq!(current.objects.get(late_equipment).zone, Zone::Battlefield);
                    assert!(current.objects.get(late_equipment).v4.attached_to.is_none());
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }

    #[test]
    fn fiery_primitive_prevention_does_not_cancel_shield_source_departure_or_control_changes() {
        for caster in [PlayerId::P0, PlayerId::P1] {
            let (mut state, creature, _, ctx) = fixture(caster, false, true);
            event::install_color_damage_prevention(
                &mut state,
                ctx.source,
                crate::mana::ManaColor::R,
            )
            .unwrap();
            // Control changes between announcement and resolution do not
            // change the captured original battlefield incarnation.
            state.players[caster.opponent().index()]
                .battlefield
                .retain(|&id| id != creature);
            state.players[caster.index()].battlefield.push(creature);
            state.objects.get_mut(creature).controller = caster;
            execute(&ctx, &mut state);
            assert_eq!(state.objects.get(creature).damage, 0);
            let replacement = state
                .engine
                .active_replacements
                .iter()
                .find(|replacement| {
                    matches!(
                        replacement.kind,
                        event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                    )
                })
                .unwrap()
                .clone();
            assert!(
                matches!(replacement.kind, event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { source, .. } if source.source == ctx.source && source.controller == caster && source.zone_change_count == 0)
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(ctx.source, Zone::Graveyard),
            );
            state.players[caster.index()]
                .battlefield
                .retain(|&id| id != creature);
            state.players[caster.opponent().index()]
                .battlefield
                .push(creature);
            state.objects.get_mut(creature).controller = caster.opponent();
            state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            event::validate_exile_if_dies_replacements(&state).unwrap();
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            );
            assert_eq!(state.objects.get(creature).zone, Zone::Exile);
        }
    }

    #[test]
    fn fiery_primitive_equipment_reattachment_stale_equipment_and_new_creature_incarnation() {
        for caster in [PlayerId::P0, PlayerId::P1] {
            for change in 0..3 {
                let (mut state, creature, equipment, ctx) = fixture(caster, true, true);
                match change {
                    0 => {
                        let other = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
                        state.attach_object_exact(equipment, 0, other, 0).unwrap();
                    }
                    1 => {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(equipment, Zone::Hand),
                        );
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(equipment, Zone::Battlefield),
                        );
                        let generation = state.objects.get(equipment).zone_change_count;
                        state
                            .attach_object_exact(equipment, generation, creature, 0)
                            .unwrap();
                    }
                    _ => {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(creature, Zone::Hand),
                        );
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(creature, Zone::Battlefield),
                        );
                        let generation = state.objects.get(creature).zone_change_count;
                        state
                            .attach_object_exact(equipment, 0, creature, generation)
                            .unwrap();
                    }
                }
                let before = serde_json::to_vec(&state).unwrap();
                execute(&ctx, &mut state);
                assert_eq!(state.objects.get(equipment).zone, Zone::Battlefield);
                if change == 2 {
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                } else {
                    assert_eq!(state.objects.get(creature).damage, 5);
                    assert!(state
                        .engine
                        .active_replacements
                        .iter()
                        .any(|replacement| matches!(
                            replacement.kind,
                            event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                        )));
                }
            }
        }
    }

    #[test]
    fn fiery_primitive_independent_protection_legality_and_invalid_pair_refusal() {
        for caster in [PlayerId::P0, PlayerId::P1] {
            for protect_creature in [false, true] {
                let (mut state, creature, equipment, ctx) = fixture(caster, true, false);
                grant(
                    &mut state,
                    if protect_creature {
                        creature
                    } else {
                        equipment
                    },
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                );
                execute(&ctx, &mut state);
                if protect_creature {
                    assert_eq!(state.objects.get(creature).damage, 0);
                    assert_eq!(state.objects.get(equipment).zone, Zone::Exile);
                    assert!(state.engine.active_replacements.is_empty());
                } else {
                    assert_eq!(state.objects.get(equipment).zone, Zone::Battlefield);
                    crate::trigger::sba_fixed_point(&mut state);
                    assert_eq!(state.objects.get(creature).zone, Zone::Exile);
                }
            }
            let (mut state, creature, _, ctx) = fixture(caster, false, false);
            grant(&mut state, creature, Keywords::HEXPROOF);
            let before = serde_json::to_vec(&state).unwrap();
            execute(&ctx, &mut state);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            for corrupt in 0..4 {
                let (mut state, _, _, mut ctx) = fixture(caster, true, false);
                match corrupt {
                    0 => ctx.target_contracts.clear(),
                    1 => {
                        ctx.targets[0] = Target::Player(caster);
                        ctx.target_contracts[0] = StackTargetContractV4::Player(caster);
                    }
                    2 => {
                        ctx.targets[1] = ctx.targets[0];
                        ctx.target_contracts[1] = ctx.target_contracts[0];
                    }
                    _ => {
                        ctx.targets.push(ctx.targets[1]);
                        ctx.target_contracts.push(ctx.target_contracts[1]);
                    }
                }
                let before = serde_json::to_vec(&state).unwrap();
                execute(&ctx, &mut state);
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
        }
    }

    #[test]
    fn fiery_primitive_cleanup_expiry_stale_target_and_replacement_receipt_refusal() {
        for caster in [PlayerId::P0, PlayerId::P1] {
            let (mut state, creature, _, ctx) = fixture(caster, false, true);
            execute(&ctx, &mut state);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(creature, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Battlefield),
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            );
            assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
            let (mut state, creature, _, ctx) = fixture(caster, false, true);
            execute(&ctx, &mut state);
            state.step = crate::state::Step::End;
            for _ in 0..8 {
                let decision = engine::advance_until_decision(&mut state);
                assert!(!matches!(decision, Decision::Halted { .. }));
                if state.step != crate::state::Step::End {
                    break;
                }
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert!(!state
                .engine
                .active_replacements
                .iter()
                .any(|replacement| matches!(
                    replacement.kind,
                    event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                )));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(creature, Zone::Graveyard),
            );
            assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
            for corrupt in 0..4 {
                let (mut state, _, _, ctx) = fixture(caster, false, true);
                execute(&ctx, &mut state);
                match corrupt {
                    0 => state.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            event::CommittedEvent::ExileIfDiesReplacementInstalledV1 { .. }
                        )
                    }),
                    1 => {
                        let event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
                            source,
                            ..
                        } = &mut state.engine.active_replacements[0].kind
                        else {
                            unreachable!()
                        };
                        source.controller = source.controller.opponent();
                    }
                    2 => {
                        let event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 {
                            target:
                                StackTargetContractV4::Object {
                                    zone_change_count, ..
                                },
                            ..
                        } = &mut state.engine.active_replacements[0].kind
                        else {
                            unreachable!()
                        };
                        *zone_change_count += 1;
                    }
                    _ => state
                        .engine
                        .active_replacements
                        .push(state.engine.active_replacements[0].clone()),
                }
                state = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                let before = serde_json::to_vec(&state).unwrap();
                assert!(engine::step(&mut state, Action::Pass).is_err());
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            }
        }
    }

    #[test]
    fn actual_installed_shield_erasure_refuses_step_and_advance_after_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _, ctx) = fixture(actor, false, true);
            execute(&ctx, &mut state);
            let mut early = state.clone();
            let replacement = early.engine.active_replacements.pop().unwrap();
            let installation = early.engine.event_history.iter().position(|e| matches!(e,
                event::CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement: r } if r == &replacement)).unwrap();
            early.engine.event_history.push(
                event::CommittedEvent::ExileIfDiesReplacementExpiredV1 {
                    replacement,
                    turn: early.turn_key_v1(),
                    active_player: actor,
                    cleanup_index: installation.try_into().unwrap(),
                },
            );
            let bytes = serde_json::to_vec(&early).unwrap();
            let mut early: GameState = serde_json::from_slice(&bytes).unwrap();
            assert!(engine::step(&mut early, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&early).unwrap(), bytes);
            state.engine.active_replacements.clear();
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert!(engine::step(&mut restored, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
            let mut expected = restored.clone();
            expected.engine.halted = Some((
                engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
            assert!(matches!(
                engine::advance_until_decision(&mut restored),
                Decision::Halted { .. }
            ));
            assert_eq!(
                serde_json::to_vec(&restored).unwrap(),
                serde_json::to_vec(&expected).unwrap()
            );
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn genuine_extra_turn_shield_uses_occurrence_and_real_cleanup_closes_once_both_seats() {
        use crate::extra_turn_v1::tests::{
            future_printed::{cast, main, next_turn, resolve},
            ready,
        };
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let source = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
            let creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            state.objects.get_mut(creature).counters.plus1_plus1 = 6;
            let forest = crate::card_def::card_id_by_name("Forest").unwrap();
            let land = state.players[actor.index()]
                .hand
                .iter()
                .copied()
                .find(|id| state.objects.get(*id).card_def == forest)
                .unwrap();
            engine::step(&mut state, Action::PlayLand(land)).unwrap();
            cast(&mut state, actor);
            resolve(&mut state);
            cast(&mut state, actor);
            resolve(&mut state);
            next_turn(&mut state);
            main(&mut state);
            assert_ne!(state.turn_key_v1(), state.turn);
            let land = state.players[actor.index()]
                .hand
                .iter()
                .copied()
                .find(|id| state.objects.get(*id).card_def == forest)
                .unwrap();
            engine::step(&mut state, Action::PlayLand(land)).unwrap();
            let mut ctx = ExecCtx::no_targets(source, actor);
            ctx.targets = vec![Target::Object(creature)];
            ctx.target_contracts = vec![StackTargetContractV4::capture(&state, ctx.targets[0])];
            // This remains a named physical Fiery opcode probe. Both extra turns
            // are genuinely scheduled by the actual printed Temporal casts.
            execute(&ctx, &mut state);
            let installed = state.engine.active_replacements.last().unwrap().clone();
            assert!(
                matches!(installed.kind, event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { turn, .. } if turn == state.turn_key_v1())
            );
            event::validate_exile_if_dies_replacements(&state).unwrap();
            let mut corrupt = state.clone();
            let old_round = corrupt.turn;
            if let event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { turn, .. } =
                &mut corrupt.engine.active_replacements.last_mut().unwrap().kind
            {
                *turn = old_round;
            }
            let forged = corrupt.engine.active_replacements.last().unwrap().clone();
            for row in &mut corrupt.engine.event_history {
                if let event::CommittedEvent::ExileIfDiesReplacementInstalledV1 { replacement } =
                    row
                {
                    *replacement = forged.clone();
                }
            }
            let bytes = serde_json::to_vec(&corrupt).unwrap();
            assert!(engine::step(&mut corrupt, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&corrupt).unwrap(), bytes);
            next_turn(&mut state);
            assert_eq!(state.active_player, actor);
            assert!(!state
                .engine
                .active_replacements
                .iter()
                .any(|r| r.id == installed.id));
            assert_eq!(state.engine.event_history.iter().filter(|e| matches!(e, event::CommittedEvent::ExileIfDiesReplacementExpiredV1 { replacement, .. } if replacement == &installed)).count(), 1);
            event::validate_exile_if_dies_replacements(&state).unwrap();
            let mut resurrected: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            resurrected.engine.active_replacements.push(installed);
            let bytes = serde_json::to_vec(&resurrected).unwrap();
            assert!(engine::step(&mut resurrected, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&resurrected).unwrap(), bytes);
        }
    }
}
