//! Genuine printed-card candidate acceptance; current registration remains unqualified.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(player: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        11033,
        player,
    );
    state.step = Step::Main1;
    state
}

#[test]
fn actual_fiery_cast_during_madness_cleanup_priority_expires_before_player_flip() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        // The library constructor leaves hands empty; physically draw this
        // fixture's opening seven before producing its real Discard(2).
        for _ in 0..7 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        }
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.objects.get_mut(target).counters.plus1_plus1 = 6;
        let temper = put(&mut state, actor, "Fiery Temper", Zone::Hand);
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        assert_eq!(state.players[actor.index()].hand.len(), 9);
        let discard_land = *state.players[actor.index()].hand.last().unwrap();
        state.step = Step::End;
        let ending_turn = state.turn_key_v1();
        let mut discarded = false;
        for _ in 0..20 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::Discard {
                    player,
                    count,
                    choices,
                } => {
                    assert_eq!((player, count), (actor, 2));
                    assert!(choices.contains(&temper) && choices.contains(&discard_land));
                    engine::step(&mut state, Action::Discard(vec![temper, discard_land])).unwrap();
                    discarded = true;
                    break;
                }
                other => panic!("actual Cleanup discard {other:?}"),
            }
        }
        assert!(discarded);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert_eq!(state.step, Step::Cleanup);
        assert!(state.stack.iter().any(|i| i.madness_offer));
        let spell = announce(&mut state, actor, target, None);
        let mut restored = copy(&state);
        for branch in [&mut state, &mut restored] {
            let mut installed = false;
            for _ in 0..20 {
                let decision = next(branch);
                if branch.objects.get(spell).zone == Zone::Graveyard {
                    assert_eq!(branch.step, Step::Cleanup);
                    assert_eq!(branch.turn_key_v1(), ending_turn);
                    assert_eq!(shield_count(branch), 1);
                    installed = true;
                    break;
                }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                engine::step(branch, Action::Pass).unwrap();
            }
            assert!(installed);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        for branch in [&mut state, &mut restored] {
            let mut declined = false;
            for _ in 0..40 {
                let decision = next(branch);
                if branch.active_player != actor {
                    break;
                }
                match decision {
                    Decision::CastSpellOrPass { .. } => engine::step(branch, Action::Pass).unwrap(),
                    Decision::ChooseMadnessCast { player, card } => {
                        assert_eq!((player, card), (actor, temper));
                        engine::step(branch, Action::ChooseMadnessCast(false)).unwrap();
                        declined = true;
                    }
                    other => panic!("actual Cleanup response {other:?}"),
                }
            }
            assert!(declined);
            assert_eq!(branch.active_player, actor.opponent());
            assert_eq!(shield_count(branch), 0);
            assert_eq!(branch.objects.get(target).damage, 0);
            assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(temper).zone, Zone::Graveyard);
            assert_eq!(
                branch
                    .engine
                    .event_history
                    .iter()
                    .filter(|e| matches!(
                        e,
                        event::CommittedEvent::ExileIfDiesReplacementExpiredV1 { .. }
                    ))
                    .count(),
                1
            );
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def =
        card_id_by_name(name).expect("reviewed Fiery Annihilation catalog admission required");
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Hand => state.players[player.index()].hand.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectOption {
                option_count: 5, ..
            } => Action::ChooseEffectOption(3), // Prismatic Strands: red
            other => panic!("unexpected Fiery Annihilation decision: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("Fiery Annihilation resolution did not settle");
}

fn copy(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn equipped(caster: PlayerId, owner: PlayerId, survives: bool) -> (GameState, ObjectId, ObjectId) {
    let mut state = ready(caster);
    let creature = put(&mut state, owner, "Llanowar Elves", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change_to_battlefield_under_controller(creature, caster),
    );
    if survives {
        state.objects.get_mut(creature).counters.plus1_plus1 = 6;
    }
    let equipment = put(&mut state, caster, "Viridian Longbow", Zone::Battlefield);
    state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 3;
    engine::step(&mut state, Action::ActivateAbility(equipment, 0)).unwrap();
    assert!(
        matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(creature)))
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    settle(&mut state);
    assert_eq!(
        state.objects.get(equipment).v4.attached_to.unwrap().object,
        creature
    );
    state.players[caster.index()].mana_pool.fill(0);
    (state, creature, equipment)
}

fn begin(state: &mut GameState, caster: PlayerId, creature: ObjectId) -> ObjectId {
    let spell = put(state, caster, "Fiery Annihilation", Zone::Hand);
    state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 3;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { can_finish: false, legal_targets, .. } if legal_targets.contains(&Target::Object(creature)))
    );
    spell
}

fn announce(
    state: &mut GameState,
    caster: PlayerId,
    creature: ObjectId,
    equipment: Option<ObjectId>,
) -> ObjectId {
    let spell = begin(state, caster, creature);
    engine::step(state, Action::ChooseTarget(Target::Object(creature))).unwrap();
    assert!(matches!(
        next(state),
        Decision::ChooseTargets {
            can_finish: true,
            ..
        }
    ));
    engine::step(
        state,
        equipment
            .map(|equipment| Action::ChooseTarget(Target::Object(equipment)))
            .unwrap_or(Action::FinishEffectSelection),
    )
    .unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == spell));
    spell
}

fn shield_count(state: &GameState) -> usize {
    state
        .engine
        .active_replacements
        .iter()
        .filter(|replacement| {
            matches!(
                replacement.kind,
                event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
            )
        })
        .count()
}

fn grant(state: &mut GameState, object: ObjectId, keywords: Keywords) {
    let generation = state.objects.get(object).zone_change_count;
    state.engine.until_end_of_turn.push(
        engine::UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
            object_id: object,
            object_zone_change_count: generation,
            layer: engine::Layers::ABILITY_ADDING,
            timestamp: 999,
            duration: engine::EffectDuration::EndOfTurn,
            keywords,
        },
    );
}

#[test]
fn fiery_printed_metadata_real_equip_optional_target_and_pending_stack_restore() {
    let def = &CARD_DEFS[usize::from(
        card_id_by_name("Fiery Annihilation").expect("actual reviewed admission required"),
    )];
    assert!(def.is_executable());
    assert_eq!(def.types, &[CardType::Instant]);
    assert_eq!(def.colors, &[ManaColor::R]);
    assert!(def.subtypes.is_empty());
    assert_eq!(def.mana_value, 3);
    assert_eq!(def.cost.generic, 2);
    assert_eq!(
        def.cost.pips,
        &[mtg_kernel::mana::Pip::Colored(ManaColor::R)]
    );
    assert_eq!(
        def.target_spec,
        TargetSpec::CreatureThenUpToOneAttachedEquipment
    );
    assert_eq!(def.target_spec.stable_id(), 64);
    let selector = mtg_kernel::rules_meaning_fiery_v1::live_selector().unwrap();
    let meaning = mtg_kernel::rules_meaning_fiery_v1::meaning(
        &selector,
        card_id_by_name("Fiery Annihilation").unwrap(),
        0,
    )
    .unwrap();
    mtg_kernel::rules_meaning_fiery_v1::validate(&meaning).unwrap();
    assert_eq!(meaning.rule.damage_count, 5);
    assert_eq!(meaning.rule.equipment_target_maximum, 1);
    for caster in [PlayerId::P0, PlayerId::P1] {
        for owner in [caster, caster.opponent()] {
            for optional in [false, true] {
                let (mut state, creature, equipment) = equipped(caster, owner, false);
                let unattached = put(&mut state, caster, "Hunter's Blowgun", Zone::Battlefield);
                let spell = begin(&mut state, caster, creature);
                let before = serde_json::to_vec(&state).unwrap();
                for invalid in [
                    Action::FinishEffectSelection,
                    Action::ChooseTarget(Target::Object(equipment)),
                ] {
                    assert!(engine::step(&mut state, invalid).is_err());
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                }
                state = copy(&state); // pending creature target
                let initial = mtg_kernel::fiery_public_v1::facts_v1(&state).unwrap();
                let context = initial.pending_cast_targets.unwrap();
                assert_eq!(
                    context.phase,
                    mtg_kernel::fiery_public_v1::CastTargetPhaseV1::MandatoryCreature
                );
                assert!(context.selected_targets.is_empty());
                assert!(!context.can_finish_without_equipment);
                for rewrite_source in [false, true] {
                    let mut forged = copy(&state);
                    let pending = forged.engine.pending_cast.as_mut().unwrap();
                    pending.target_spec = TargetSpec::None;
                    if rewrite_source {
                        pending.source_contract.card_def =
                            card_id_by_name("Lightning Bolt").unwrap();
                    }
                    let forged = copy(&forged);
                    let before = serde_json::to_vec(&forged).unwrap();
                    assert!(mtg_kernel::fiery_public_v1::facts_v1(&forged).is_err());
                    assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
                }
                engine::step(&mut state, Action::ChooseTarget(Target::Object(creature))).unwrap();
                let public = mtg_kernel::fiery_public_v1::facts_v1(&state).unwrap();
                let context = public.pending_cast_targets.as_ref().unwrap();
                assert_eq!(
                    context.phase,
                    mtg_kernel::fiery_public_v1::CastTargetPhaseV1::OptionalAttachedEquipment
                );
                assert_eq!(context.selected_targets.len(), 1);
                assert_eq!(context.selected_targets[0].arena_id, creature.0);
                assert!(context.can_finish_without_equipment);
                assert_eq!(
                    mtg_kernel::fiery_public_v1::facts_v1(&copy(&state)).unwrap(),
                    public
                );
                assert!(
                    matches!(next(&mut state), Decision::ChooseTargets { can_finish: true, legal_targets, .. } if legal_targets == vec![Target::Object(equipment)])
                );
                let before = serde_json::to_vec(&state).unwrap();
                for invalid in [
                    Target::Object(unattached),
                    Target::Object(creature),
                    Target::Player(caster),
                ] {
                    assert!(engine::step(&mut state, Action::ChooseTarget(invalid)).is_err());
                    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                }
                let mut restored = copy(&state); // pending optional Equipment target
                for current in [&mut state, &mut restored] {
                    engine::step(
                        current,
                        if optional {
                            Action::ChooseTarget(Target::Object(equipment))
                        } else {
                            Action::FinishEffectSelection
                        },
                    )
                    .unwrap();
                    assert!(matches!(next(current), Decision::CastSpellOrPass { .. }));
                    let mut stack_restored = copy(current);
                    settle(current);
                    settle(&mut stack_restored);
                    assert_eq!(
                        current.diagnostic_state_hash(),
                        stack_restored.diagnostic_state_hash()
                    );
                    assert_eq!(current.objects.get(creature).zone, Zone::Exile);
                    assert_eq!(
                        current.objects.get(equipment).zone,
                        if optional {
                            Zone::Exile
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert!(current.objects.get(equipment).v4.attached_to.is_none());
                    assert_eq!(current.objects.get(unattached).zone, Zone::Battlefield);
                    assert_eq!(current.objects.get(spell).zone, Zone::Graveyard);
                    assert!(current.engine.event_history.iter().any(|event| matches!(event, event::CommittedEvent::Damage { source, target: Target::Object(target), amount: 5 } if *source == spell && *target == creature)));
                    assert_eq!(
                        current.players[caster.index()].mana_pool[ManaColor::R.pool_index()],
                        0
                    );
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    restored.diagnostic_state_hash()
                );
            }
        }
    }
}

#[test]
fn fiery_actual_strands_prevention_still_exiles_equipment_and_later_death_after_source_departure() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature, equipment) = equipped(caster, caster.opponent(), false);
        let spell = announce(&mut state, caster, creature, Some(equipment));
        let strands = put(&mut state, caster, "Prismatic Strands", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 3;
        engine::step(&mut state, Action::CastSpell(strands)).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        state = copy(&state);
        settle(&mut state);
        assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(creature).damage, 0);
        assert_eq!(state.objects.get(equipment).zone, Zone::Exile);
        assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        assert_eq!(shield_count(&state), 1);
        let public = mtg_kernel::fiery_public_v1::facts_v1(&state).unwrap();
        assert_eq!(public.death_replacements.len(), 1);
        let row = &public.death_replacements[0];
        assert_eq!(row.source_at_resolution.arena_id, spell.0);
        assert_eq!(row.original_target.arena_id, creature.0);
        assert!(row.original_target_present);
        assert_eq!(
            row.lifecycle,
            mtg_kernel::fiery_public_v1::LifecycleV1::Unexpired
        );
        assert_eq!(
            mtg_kernel::fiery_public_v1::facts_v1(&copy(&state)).unwrap(),
            public
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        );
        assert_eq!(state.objects.get(creature).zone, Zone::Exile);
        let departed = mtg_kernel::fiery_public_v1::facts_v1(&state).unwrap();
        assert!(!departed.death_replacements[0].original_target_present);
        assert_eq!(
            departed.death_replacements[0].original_target,
            row.original_target
        );
    }
}

#[test]
fn fiery_acquired_protection_allows_only_independently_legal_targets_and_fizzle_without_shield() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for protect in 0..3 {
            let (mut state, creature, equipment) = equipped(caster, caster.opponent(), false);
            let spell = announce(&mut state, caster, creature, Some(equipment));
            if protect != 1 {
                grant(&mut state, creature, Keywords::PROTECTION_FROM_MONOCOLORED);
            }
            if protect != 0 {
                grant(&mut state, equipment, Keywords::PROTECTION_FROM_MONOCOLORED);
            }
            state = copy(&state);
            settle(&mut state);
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
            if protect == 1 {
                assert_eq!(state.objects.get(creature).zone, Zone::Exile);
                assert_eq!(state.objects.get(equipment).zone, Zone::Battlefield);
                assert_eq!(shield_count(&state), 1);
            } else {
                assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
                assert_eq!(state.objects.get(creature).damage, 0);
                assert_eq!(shield_count(&state), 0);
                assert_eq!(
                    state.objects.get(equipment).zone,
                    if protect == 0 {
                        Zone::Exile
                    } else {
                        Zone::Battlefield
                    }
                );
            }
        }
        let mut state = ready(caster);
        put(
            &mut state,
            caster.opponent(),
            "Guardian of the Guildpact",
            Zone::Battlefield,
        );
        let spell = put(&mut state, caster, "Fiery Annihilation", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 3;
        assert!(
            !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[test]
fn fiery_attachment_change_equipment_departure_and_creature_reentry_recheck_captured_targets() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for change in 0..3 {
            let (mut state, creature, equipment) = equipped(caster, caster.opponent(), true);
            let spell = announce(&mut state, caster, creature, Some(equipment));
            match change {
                0 => {
                    // An attachment event fixture, represented by the same exact
                    // bidirectional relations as a resolved Equip. No instant
                    // speed Equip permission is implied.
                    state
                        .objects
                        .get_mut(creature)
                        .attachments
                        .retain(|&id| id != equipment);
                    state.objects.get_mut(equipment).v4.attached_to = None;
                }
                1 => event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(equipment, Zone::Hand),
                ),
                _ => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(creature, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change_to_battlefield_under_controller(
                            creature, caster,
                        ),
                    );
                }
            }
            let creature_generation = state.objects.get(creature).zone_change_count;
            state = copy(&state);
            settle(&mut state);
            assert_eq!(state.objects.get(creature).zone, Zone::Battlefield);
            assert_eq!(
                state.objects.get(creature).zone_change_count,
                creature_generation
            );
            assert_eq!(
                state.objects.get(creature).damage,
                if change == 2 { 0 } else { 5 }
            );
            assert_eq!(shield_count(&state), if change == 2 { 0 } else { 1 });
            assert_eq!(
                state.objects.get(equipment).zone,
                if change == 1 {
                    Zone::Hand
                } else {
                    Zone::Battlefield
                }
            );
            assert_eq!(state.objects.get(spell).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn fiery_cleanup_expires_replacement_and_restored_forgery_refuses_before_mutation() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let (mut state, creature, _) = equipped(caster, caster.opponent(), true);
        announce(&mut state, caster, creature, None);
        settle(&mut state);
        assert_eq!(shield_count(&state), 1);
        let mut forged = copy(&state);
        let replacement = forged
            .engine
            .active_replacements
            .iter_mut()
            .find(|replacement| {
                matches!(
                    replacement.kind,
                    event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { .. }
                )
            })
            .unwrap();
        let event::ReplacementEffectKind::ExileIfDiesUntilEndOfTurnV1 { source, .. } =
            &mut replacement.kind
        else {
            unreachable!()
        };
        source.controller = source.controller.opponent();
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(mtg_kernel::fiery_public_v1::facts_v1(&forged).is_err());
        assert!(engine::step(&mut forged, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        state.step = Step::End;
        for _ in 0..8 {
            next(&mut state);
            if state.step != Step::End {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(shield_count(&state), 0);
        assert_eq!(state.objects.get(creature).damage, 0);
        let public = mtg_kernel::fiery_public_v1::facts_v1(&state).unwrap();
        assert_eq!(public.death_replacements.len(), 1);
        assert_eq!(
            public.death_replacements[0].lifecycle,
            mtg_kernel::fiery_public_v1::LifecycleV1::ExpiredAtCleanup
        );
        state = copy(&state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(creature, Zone::Graveyard),
        );
        assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
    }
}
