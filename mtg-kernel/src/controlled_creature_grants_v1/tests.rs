use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state_for};

#[test]
fn registered_absence_preserves_state_hash_observation_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let bytes = serde_json::to_vec(&state).unwrap();
        let hash = state.diagnostic_state_hash();
        let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
        let obs = crate::rl::observe_policy_v6(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert!(!grants_keyword(&state, elf, Keywords::FLYING));
        assert_eq!(color_protection(&state, elf), 0);
        require_legacy_consumer_supported(&state).unwrap();
        assert!(!detach_protected_nonauras(&mut state));
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        assert_eq!(state.diagnostic_state_hash(), hash);
        let restored: GameState = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            serde_json::to_vec(
                &crate::rl::observe_policy_v6(&restored, &surface, actor, 0, 0, 0, 1).unwrap()
            )
            .unwrap(),
            serde_json::to_vec(&obs).unwrap()
        );
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::card_def::{card_id_by_name, Supertype, TargetSpec};
    use crate::effect::{self, EffectOp, ExecCtx};
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ProposedEvent};
    use crate::mana::ManaColor;
    use crate::state::{Step, Target};

    fn next(state: &mut GameState) -> Decision {
        let d = engine::advance_until_decision(state);
        assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
        d
    }
    fn settle(state: &mut GameState) {
        for _ in 0..40 {
            let d = next(state);
            if state.stack.is_empty() && state.engine.pending_cast.is_none() {
                assert!(matches!(d, Decision::CastSpellOrPass { .. }));
                return;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(state, Action::Pass).unwrap();
        }
        panic!("actual stack did not settle");
    }
    fn ready(actor: PlayerId) -> GameState {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            put(&mut state, player, "Lightning Bolt", Zone::Hand);
        }
        state
    }
    fn enter(state: &mut GameState, actor: PlayerId) -> ObjectId {
        let source = put(state, actor, "Akroma's Memorial", Zone::Hand);
        event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
        source
    }
    fn restore(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }
    fn assert_grants(state: &GameState, creature: ObjectId, expected: bool) {
        for kw in [
            Keywords::FLYING,
            Keywords::FIRST_STRIKE,
            Keywords::VIGILANCE,
            Keywords::TRAMPLE,
            Keywords::HASTE,
        ] {
            assert_eq!(engine::has_effective_keyword(state, creature, kw), expected);
        }
        assert_eq!(
            color_protection(state, creature),
            if expected { 12 } else { 0 }
        );
        for (colors, protected) in [
            (4, true),
            (8, true),
            (12, true),
            (9, true),
            (6, true),
            (1, false),
            (2, false),
            (16, false),
            (0, false),
        ] {
            assert_eq!(
                protects_from_colors(state, creature, colors),
                expected && protected
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_seven_mana_cast_five_keywords_two_protections_late_entry_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let other = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let source = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
            let def = &CARD_DEFS[usize::from(card_id_by_name("Akroma's Memorial").unwrap())];
            assert!(def.is_executable());
            assert_eq!(def.cost.generic, 7);
            assert!(def.cost.pips.is_empty());
            assert_eq!(def.types, &[CardType::Artifact]);
            assert_eq!(def.supertypes, &[Supertype::Legendary]);
            assert!(def.colors.is_empty());
            assert_eq!(def.controlled_creature_grant, Some(MEMORIAL));
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.objects.get(source).zone, Zone::Stack);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                settle(branch);
                assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
                assert_grants(branch, elf, true);
                assert_grants(branch, other, false);
                let core = crate::rl_session::FastActorSessionV1::from_akroma_v40_fixture_state(
                    branch.clone(),
                );
                let crate::rl_session::FastActorResponseV1::Decision(expected) =
                    core.current_response()
                else {
                    panic!("real Akroma priority");
                };
                let seat = expected.acting_player;
                let session =
                    crate::human_bo3_v1::HumanAkromaSessionV40::from_session(core, seat).unwrap();
                let public = session.public_observation(expected).unwrap();
                assert_eq!(public.roots.len(), 2);
                assert_eq!(public.grants.len(), 1);
                assert_eq!(public.grants[0].source.arena_id, source.0);
                assert!(public.grants[0]
                    .recipients
                    .iter()
                    .any(|row| row.arena_id == elf.0));
                assert!(!public.grants[0]
                    .recipients
                    .iter()
                    .any(|row| row.arena_id == other.0));
                assert_eq!(public.grants[0].keyword_mask, 61);
                assert_eq!(public.grants[0].protection_color_mask, 12);
                let mut projector = crate::human_bo3_v1::HumanAkromaProjectorV40::new(seat);
                let visible = projector.project_current(&session, expected).unwrap();
                assert_eq!(visible.state.extensions.grants[0].protection_color_mask, 12);
                let serialized = serde_json::to_string(&visible).unwrap();
                assert!(!serialized.contains("private_snapshot_json"));
                assert!(!serialized.contains("arena_id"));

                let late = put(branch, actor, "Llanowar Elves", Zone::Hand);
                event::propose_and_commit(
                    branch,
                    ProposedEvent::zone_change(late, Zone::Battlefield),
                );
                assert_grants(branch, late, true);
                assert!(engine::can_attack(branch, late));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn actual_flash_response_fizzles_announced_bolt_and_refuses_old_projections_readonly() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
            let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            state.priority_player = actor.opponent();
            state.players[actor.opponent().index()].mana_pool[ManaColor::R.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(elf))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            let source = put(&mut state, actor, "Akroma's Memorial", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 7;
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            next(&mut state);
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                settle(branch);
                assert_eq!(branch.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(elf).damage, 0);
                assert_grants(branch, elf, true);
                let before = serde_json::to_vec(branch).unwrap();
                for viewer in [actor, actor.opponent()] {
                    assert!(crate::rl::observe_v1(branch, viewer, 0).is_err());
                    assert!(crate::rl::observe_policy_v6(
                        branch,
                        &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                        viewer,
                        0,
                        0,
                        0,
                        1
                    )
                    .is_err());
                }
                assert_eq!(serde_json::to_vec(branch).unwrap(), before);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn debt_real_black_equipment_targeting_damage_and_prevention_override_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let rod = put(&mut state, actor, "Black Mage's Rod", Zone::Battlefield);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
            assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(elf))).unwrap();
            settle(&mut state);
            assert!(state.objects.get(elf).attachments.contains(&rod));
            enter(&mut state, actor);
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                crate::trigger::sba_fixed_point(branch);
                assert_eq!(branch.objects.get(rod).zone, Zone::Battlefield);
                assert!(branch.objects.get(rod).v4.attached_to.is_none());
                assert!(!branch.objects.get(elf).attachments.contains(&rod));
                let source = put(branch, actor.opponent(), "Lightning Bolt", Zone::Hand);
                assert!(!engine::effect_target_is_legal(
                    branch,
                    source,
                    actor.opponent(),
                    TargetSpec::Creature,
                    &[Target::Object(elf)],
                    0
                ));
                event::propose_and_commit(
                    branch,
                    ProposedEvent::damage(source, Target::Object(elf), 1),
                );
                assert_eq!(branch.objects.get(elf).damage, 0);
                effect::execute(
                    &EffectOp::DamageCannotBePreventedThisTurn,
                    &ExecCtx::no_targets(source, actor.opponent()),
                    branch,
                );
                event::propose_and_commit(
                    branch,
                    ProposedEvent::damage(source, Target::Object(elf), 1),
                );
                assert_eq!(branch.objects.get(elf).damage, 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn current_control_layer_timestamp_departure_reentry_and_restored_queries() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let source = enter(&mut state, actor);
            let witness = put(
                &mut state,
                actor.opponent(),
                "Witness Protection",
                Zone::Hand,
            );
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(witness, Zone::Battlefield),
            );
            state
                .attach_object_exact(
                    witness,
                    state.objects.get(witness).zone_change_count,
                    elf,
                    state.objects.get(elf).zone_change_count,
                )
                .unwrap();
            assert_grants(&state, elf, false);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert_grants(&state, elf, true);
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                // Physical controller-transfer primitive; no claim of a printed theft spell.
                branch.players[actor.index()]
                    .battlefield
                    .retain(|id| *id != source);
                branch.players[actor.opponent().index()]
                    .battlefield
                    .push(source);
                branch.objects.get_mut(source).controller = actor.opponent();
                assert_grants(branch, elf, false);
                event::propose_and_commit(
                    branch,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                assert_grants(branch, elf, false);
                assert!(require_legacy_consumer_supported(branch).is_err());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_blocker_validation_refuses_flying_black_red_and_allows_blue_restore() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready(actor);
            let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            enter(&mut state, actor);
            let red = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            let black = put(
                &mut state,
                actor.opponent(),
                "Billowing Shriekmass",
                Zone::Battlefield,
            );
            let blue = put(
                &mut state,
                actor.opponent(),
                "High Fae Trickster",
                Zone::Battlefield,
            );
            state.step = Step::DeclareAttackers;
            engine::step(&mut state, Action::DeclareAttackers(vec![elf])).unwrap();
            for _ in 0..20 {
                let d = next(&mut state);
                if matches!(d, Decision::DeclareBlockers { .. }) {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }));
                engine::step(&mut state, Action::Pass).unwrap();
            }
            assert_eq!(state.step, Step::DeclareBlockers);
            assert!(!state.objects.get(elf).tapped);
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                for forbidden in [red, black] {
                    let before = serde_json::to_vec(branch).unwrap();
                    assert!(
                        engine::step(branch, Action::DeclareBlockers(vec![(forbidden, elf)]))
                            .is_err()
                    );
                    assert_eq!(serde_json::to_vec(branch).unwrap(), before);
                }
                engine::step(branch, Action::DeclareBlockers(vec![(blue, elf)])).unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn prebatch_color_lki_preserves_green_original_after_reentry_and_missing_receipts_refuse() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for witness_first in [false, true] {
                let mut state = ready(actor);
                enter(&mut state, actor);
                let source = put(&mut state, actor, "Goblin Boarders", Zone::Battlefield);
                let original = crate::state::AbilitySourceContractV4::capture(&state, source);
                let witness = put(
                    &mut state,
                    actor.opponent(),
                    "Witness Protection",
                    Zone::Hand,
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(witness, Zone::Battlefield),
                );
                state
                    .attach_object_exact(
                        witness,
                        state.objects.get(witness).zone_change_count,
                        source,
                        state.objects.get(source).zone_change_count,
                    )
                    .unwrap();
                assert_eq!(engine::object_color_mask(&state, source), 16);
                // Genuine simultaneous zone producer; this is a color-LKI
                // primitive setup, not a claimed mass-removal printed spell.
                let ids = if witness_first {
                    [witness, source]
                } else {
                    [source, witness]
                };
                let moves = ids
                    .map(|id| ProposedEvent::zone_change(id, Zone::Hand))
                    .to_vec();
                event::propose_and_commit_batch(&mut state, moves);
                validate_color_history(&state).unwrap();
                assert_eq!(
                    targeting_source_colors(
                        &state,
                        original.source,
                        original.card_def,
                        original.zone,
                        original.zone_change_count
                    )
                    .unwrap(),
                    16
                );
                let clean = serde_json::to_vec(&state).unwrap();
                for mode in 0..6 {
                    let mut bad: GameState = serde_json::from_slice(&clean).unwrap();
                    if mode == 0 {
                        bad.engine.event_history.retain(|event| {
                            !matches!(
                                event,
                                event::CommittedEvent::ProtectionSourceColorDepartureV1 { .. }
                            )
                        });
                    } else {
                        let row = bad.engine.event_history.iter_mut().find(|event|
                            matches!(event, event::CommittedEvent::ProtectionSourceColorDepartureV1 {
                                snapshot, ..
                            } if snapshot.source.source == source)).unwrap();
                        let event::CommittedEvent::ProtectionSourceColorDepartureV1 {
                            snapshot,
                            departure_history_index,
                        } = row
                        else {
                            unreachable!()
                        };
                        match mode {
                            1 => *departure_history_index += 1,
                            2 => snapshot.source.owner = actor.opponent(),
                            3 => snapshot.source.zone_change_count += 1,
                            4 => snapshot.private_snapshot_json.clear(),
                            5 => {
                                let mut original: crate::state::GameObject =
                                    serde_json::from_str(&snapshot.private_snapshot_json).unwrap();
                                original.card_def = card_id_by_name("Forest").unwrap();
                                snapshot.private_snapshot_json =
                                    serde_json::to_string(&original).unwrap();
                            }
                            _ => unreachable!(),
                        }
                    }
                    let mut restored = restore(&bad);
                    for branch in [&mut bad, &mut restored] {
                        let before = serde_json::to_vec(branch).unwrap();
                        assert!(engine::step(branch, Action::Pass).is_err());
                        assert_eq!(serde_json::to_vec(branch).unwrap(), before);
                        let mut expected = restore(branch);
                        expected.engine.halted = Some((
                            engine::UnsupportedMechanic::InvalidEffectContinuation,
                            ObjectId(0),
                        ));
                        assert!(matches!(
                            engine::advance_until_decision(branch),
                            Decision::Halted { .. }
                        ));
                        assert_eq!(
                            serde_json::to_vec(branch).unwrap(),
                            serde_json::to_vec(&expected).unwrap()
                        );
                    }
                }
                let mut restored = restore(&state);
                for branch in [&mut state, &mut restored] {
                    event::propose_and_commit(
                        branch,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                    assert_eq!(engine::object_color_mask(branch, source), 8);
                    assert_eq!(
                        targeting_source_colors(
                            branch,
                            original.source,
                            original.card_def,
                            original.zone,
                            original.zone_change_count
                        )
                        .unwrap(),
                        16
                    );
                    validate_color_history(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
        }
    }
}
