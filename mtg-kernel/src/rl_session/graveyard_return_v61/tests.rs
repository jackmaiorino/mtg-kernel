//! Genuine paid native stages through the sealed V61 owner. Runtime unqualified.
use super::*;
use crate::{
    card_def, engine,
    graveyard_return_public_v61::{tests as native, PowerSourceV61, ProgramV61},
    ids::ObjectId,
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::Zone,
};
fn own(state: GameState) -> GraveyardReturnSessionV61 {
    GraveyardReturnSessionV61::from_session(
        FastActorSessionV1::from_graveyard_return_v61_fixture_state(state),
    )
    .unwrap()
}
fn world(owner: &GraveyardReturnSessionV61) -> &GameState {
    owner.fixture_state_v61()
}
fn taps(owner: &GraveyardReturnSessionV61, actor: PlayerId) -> usize {
    world(owner).players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| world(owner).objects.get(id).tapped)
        .count()
}
fn act(owner: &mut GraveyardReturnSessionV61, predicate: impl Fn(&A) -> bool) {
    let decision = owner
        .decision_v61()
        .unwrap_or_else(|e| panic!("{e:?}; {:?}", owner.response()));
    let public = decision
        .actions()
        .iter()
        .position(predicate)
        .unwrap_or_else(|| panic!("actual V61 action absent: {:?}", decision.actions()));
    let raw = decision.fixture_original_index_v61(public as u32).unwrap();
    assert_eq!(
        decision.binding.original,
        owner.inner.current.as_ref().unwrap().candidates
    );
    let current = &owner.inner.current.as_ref().unwrap().origin_decision;
    if matches!(
        current,
        PolicyDecisionV5::AttackerInclusion { .. } | PolicyDecisionV5::BlockerInclusion { .. }
    ) {
        let feasible = owner
            .inner
            .surface
            .feasible_scan_answers(world(owner), current)
            .unwrap();
        assert_eq!(
            decision.actions().len(),
            feasible.iter().filter(|b| **b).count()
        );
        for (i, row) in decision.actions().iter().enumerate() {
            let include = match row {
                A::ChooseAttackerInclusion { include, .. }
                | A::ChooseBlockerInclusion { include, .. } => *include,
                _ => panic!("scan changed family"),
            };
            assert!(feasible[usize::from(include)]);
            assert_eq!(
                decision.binding.original
                    [decision.fixture_original_index_v61(i as u32).unwrap() as usize]
                    .semantic,
                *row
            );
        }
    }
    owner.validate_decision_v61(&decision).unwrap();
    let mut reference = owner.fixture_native_v61();
    let expected = decision.expected();
    let response = owner.step_v61(&decision, public as u32).unwrap();
    let native = reference
        .step(expected.episode_id, expected.step, raw)
        .unwrap();
    assert_eq!(response, native);
    assert_eq!(world(owner), reference.graveyard_return_fixture_state());
    assert_eq!(
        world(owner).state_hash(),
        reference.graveyard_return_fixture_state().state_hash()
    );
    assert_eq!(
        owner.fixture_counts_v61(),
        (
            reference.policy_step_count(),
            reference.physical_decision_count()
        )
    );
}
fn quiet(owner: &mut GraveyardReturnSessionV61) {
    let d = owner.decision_v61().unwrap();
    let index = d
        .actions()
        .iter()
        .position(|a| matches!(a, A::Pass { .. }))
        .or_else(|| {
            d.actions().iter().position(|a| {
                matches!(
                    a,
                    A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            })
        })
        .or_else(|| {
            d.actions()
                .iter()
                .position(|a| matches!(a, A::ChooseEffectBoolean { value: true, .. }))
        })
        .unwrap_or(0);
    let selected = d.actions()[index].clone();
    act(owner, |a| a == &selected);
}
fn drain(owner: &mut GraveyardReturnSessionV61) {
    for _ in 0..160 {
        let s = world(owner);
        if s.stack.is_empty()
            && s.engine.pending_cast.is_none()
            && s.engine.pending_activation.is_none()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(owner);
    }
    panic!("genuine V61 paid native resolution did not drain");
}
fn cast(
    owner: &mut GraveyardReturnSessionV61,
    source: ObjectId,
    target: Option<ObjectId>,
    cost: usize,
) {
    let actor = world(owner).objects.get(source).owner;
    for _ in 0..20 {
        if owner
            .decision_v61()
            .unwrap()
            .actions()
            .iter()
            .any(|a| matches!(a,A::CastSpell{source:row,..}if row.arena_id==source.0))
        {
            break;
        }
        quiet(owner);
    }
    let before = taps(owner, actor);
    act(
        owner,
        |a| matches!(a,A::CastSpell{source:row,..}if row.arena_id==source.0),
    );
    let modes = owner.decision_v61().unwrap();
    if modes
        .actions()
        .iter()
        .any(|a| matches!(a, A::ChooseSpellMode { .. }))
    {
        // Employment's real Treasure makes both Abrade modes available.
        // Choose the actual creature-damage mode before the original target.
        assert_eq!(
            card_def::CARD_DEFS[usize::from(world(owner).objects.get(source).card_def)].name,
            "Abrade"
        );
        assert_eq!(modes.actions().len(), 2);
        for mode_index in 0..2 {
            assert!(modes.actions().iter().any(|a| matches!(a,
                A::ChooseSpellMode { actor: seat, source: row, mode_index: mode, mode_count: 2 }
                    if *seat == actor.into() && row.arena_id == source.0 && *mode == mode_index)));
        }
        act(owner, |a| {
            matches!(a, A::ChooseSpellMode { mode_index: 0, .. })
        });
    }
    if let Some(target) = target {
        act(
            owner,
            |a| matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0),
        );
    }
    assert!(world(owner).engine.pending_cast.is_none());
    assert_eq!(world(owner).objects.get(source).zone, Zone::Stack);
    assert_eq!(taps(owner, actor) - before, cost);
    assert_eq!(world(owner).players[actor.index()].mana_pool, [0; 6]);
}
fn check_restore(owner: &mut GraveyardReturnSessionV61) {
    let decision = owner.decision_v61().unwrap();
    let snapshot = owner.snapshot_v61();
    let before = world(owner).clone();
    let counts = owner.fixture_counts_v61();
    let packet = decision.candidates_v61().clone();
    owner.restore_v61(&snapshot).unwrap();
    assert_eq!(world(owner), &before);
    assert_eq!(owner.fixture_counts_v61(), counts);
    assert!(owner.validate_decision_v61(&decision).is_err());
    assert_eq!(owner.decision_v61().unwrap().candidates_v61(), &packet);
}
#[test]
fn genuine_paid_cast_eight_revival_death_schedule_next_end_step_full_native_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        let removal = put(&mut state, actor, "Abrade", Zone::Hand);
        let mut owner = own(state);
        cast(&mut owner, familiar, None, 3);
        drain(&mut owner);
        assert_eq!(world(&owner).objects.get(familiar).counters.revival, 8);
        let d = owner.decision_v61().unwrap();
        let row = d.observation().graveyard_return.battlefield_counters[actor.index()]
            .iter()
            .find(|r| r.card.object == familiar.0)
            .unwrap();
        assert_eq!(row.counters.revival, 8);
        assert!(
            !d.observation().owning_consumer_admitted
                && !d.candidates_v61().owning_consumer_admitted
        );
        assert!(d.candidates_v61().alesha_optional_resolution_supported);
        assert!(crate::next_five_observation_v59::validate_state(world(&owner)).is_err());
        check_restore(&mut owner);
        cast(&mut owner, removal, Some(familiar), 2);
        drain(&mut owner);
        assert_eq!(world(&owner).objects.get(familiar).zone, Zone::Graveyard);
        let d = owner.decision_v61().unwrap();
        assert_eq!(
            d.observation().graveyard_return.nine_lives_schedules[0].departure_revival,
            8
        );
        check_restore(&mut owner);
        for _ in 0..250 {
            if world(&owner).objects.get(familiar).zone == Zone::Battlefield {
                break;
            }
            quiet(&mut owner);
        }
        assert_eq!(world(&owner).objects.get(familiar).zone, Zone::Battlefield);
        assert_eq!(world(&owner).objects.get(familiar).counters.revival, 7);
        check_restore(&mut owner);
    }
}
#[test]
fn genuine_paid_alesha_attack_target_response_removal_optional_accept_decline_and_lki_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            for remove in [false, true] {
                let mut state = native::ready(actor);
                let alesha = put(&mut state, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
                let target = put(&mut state, actor, "Vampire Nighthawk", Zone::Hand);
                let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
                let downfall = put(&mut state, actor.opponent(), "Hero's Downfall", Zone::Hand);
                let mut owner = own(state);
                cast(&mut owner, alesha, None, 3);
                drain(&mut owner);
                cast(&mut owner, target, None, 3);
                drain(&mut owner);
                cast(&mut owner, abrade, Some(target), 2);
                drain(&mut owner);
                let original_turn = world(&owner).turn;
                let mut attacked = false;
                for _ in 0..400 {
                    let d = owner.decision_v61().unwrap();
                    if world(&owner).turn>original_turn && d.actions().iter().any(|a|matches!(a,A::ChooseAttackerInclusion{attacker,include:true,..}if attacker.arena_id==alesha.0)){
                act(&mut owner,|a|matches!(a,A::ChooseAttackerInclusion{attacker,include:true,..}if attacker.arena_id==alesha.0));attacked=true;break;
            }
                    quiet(&mut owner);
                }
                assert!(attacked);
                for _ in 0..240 {
                    if owner.decision_v61().unwrap().actions().iter().any(|a|matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0)){break;}
                    quiet(&mut owner);
                }
                assert_eq!(
                    engine::checked_effective_pt(world(&owner), alesha)
                        .unwrap()
                        .0,
                    3
                );
                act(
                    &mut owner,
                    |a| matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0),
                );
                assert!(world(&owner).stack.iter().any(|item| item.source == alesha));
                if remove {
                    cast(&mut owner, downfall, Some(alesha), 3);
                }
                for _ in 0..100 {
                    if owner.decision_v61().unwrap().actions().iter().any(|a|matches!(a,A::ChooseEffectBoolean{source,..}if source.arena_id==alesha.0)){break;}
                    quiet(&mut owner);
                }
                let d = owner.decision_v61().unwrap();
                assert!(d
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { value: false, .. })));
                assert!(d
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { value: true, .. })));
                assert!(d.observation().graveyard_return.triggers.iter().any(|r|matches!(&r.program,ProgramV61::AleshaRaid{power,..}if power.amount==3 && power.source==if remove{PowerSourceV61::ExactDepartureLki}else{PowerSourceV61::CurrentOriginalIncarnation})));
                check_restore(&mut owner);
                let bytes = serde_json::to_vec(world(&owner)).unwrap();
                let restored: GameState = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(restored, *world(&owner));
                let mut reconstructed = own(restored);
                act(
                    &mut reconstructed,
                    |a| matches!(a,A::ChooseEffectBoolean{value,..}if *value==accept),
                );
                drain(&mut reconstructed);
                act(
                    &mut owner,
                    |a| matches!(a,A::ChooseEffectBoolean{value,..}if *value==accept),
                );
                drain(&mut owner);
                assert_eq!(world(&owner), world(&reconstructed));
                assert_eq!(
                    world(&owner).objects.get(target).zone,
                    if accept {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(
                    world(&owner).objects.get(alesha).zone,
                    if remove {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
            }
        }
    }
}
#[test]
fn sealed_owner_foreign_stale_range_packet_mapping_counter_and_callback_forgeries_refuse_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        native::paid(&mut state, actor, "Nine-Lives Familiar", None);
        let mut owner = own(state.clone());
        let foreign = own(state);
        let decision = owner.decision_v61().unwrap();
        let before = world(&owner).clone();
        let counts = owner.fixture_counts_v61();
        assert!(owner
            .validate_decision_v61(&foreign.decision_v61().unwrap())
            .is_err());
        assert!(owner.restore_v61(&foreign.snapshot_v61()).is_err());
        assert!(owner.step_v61(&decision, u32::MAX).is_err());
        for kind in 0..5 {
            let mut forged = decision.clone();
            match kind {
                0 => {
                    forged
                        .binding
                        .projection
                        .observation
                        .graveyard_return
                        .battlefield_counters[actor.index()][0]
                        .counters
                        .revival += 1
                }
                1 => forged.binding.public_to_original[0] = usize::MAX,
                2 => {
                    forged.binding.original.remove(0);
                }
                3 => forged.actions.clear(),
                _ => forged.binding.expected.environment_revision += 1,
            }
            assert!(owner.validate_decision_v61(&forged).is_err());
            assert!(owner.step_v61(&forged, 0).is_err());
            assert_eq!(world(&owner), &before);
            assert_eq!(owner.fixture_counts_v61(), counts);
        }
        let result = owner.step_with_current_check(
            &decision,
            0,
            |_, _| Err::<(), _>(RlContractError("callback refused".into())),
            |e| e,
            |e| e,
        );
        assert!(result.is_err());
        assert_eq!(world(&owner), &before);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<_, RlContractError> = owner.step_with_current_check(
                &decision,
                0,
                |_, _| panic!("readonly callback"),
                |e| e,
                |e| e,
            );
        }));
        assert!(panic.is_err());
        assert_eq!(world(&owner), &before);
        assert_eq!(owner.fixture_counts_v61(), counts);
        let mut invalid = owner.snapshot_v61();
        invalid.inner.0.current.as_mut().unwrap().candidates.clear();
        assert!(owner.restore_v61(&invalid).is_err());
        assert_eq!(world(&owner), &before);
        check_restore(&mut owner);
        assert!(owner.validate_decision_v61(&decision).is_err());
    }
}
#[test]
fn genuine_paid_drake_issued_origin_preserves_full_world_and_rejects_foreign_borrow_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        let hatcher = put(&mut state, actor, "Drake Hatcher", Zone::Hand);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        let mut owner = own(state);
        cast(&mut owner, hatcher, None, 2);
        drain(&mut owner);
        cast(&mut owner, familiar, None, 3);
        drain(&mut owner);
        let before = world(&owner).clone();
        let d = owner.decision_v61().unwrap();
        owner.validate_decision_v61(&d).unwrap();
        assert_eq!(world(&owner), &before);
        let current = owner.inner.current.as_ref().unwrap();
        let proof = IssuedOriginV61 {
            session: &owner.inner,
            current,
        };
        let foreign = owner.inner.clone();
        assert!(proof
            .validate(
                &foreign.state,
                &foreign.surface,
                (current.actor, &current.origin_decision),
                owner.inner.policy_step_count,
                current.physical_decision_id,
                (current.substep_index, current.substep_count)
            )
            .is_err());
        // The source-only exact probe still refuses this real Enclosing replay;
        // only the private issued-origin borrow authenticates the owned stream.
        assert!(candidates::project_v61(
            &owner.inner.state,
            &owner.inner.surface,
            (current.actor, &current.origin_decision),
            owner.inner.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count)
        )
        .is_err());
        check_restore(&mut owner);
    }
}
#[test]
fn independent_hidden_worlds_and_unsupported_public_later_families_preserve_closed_scope_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut base = native::ready(actor);
        native::paid(&mut base, actor, "Nine-Lives Familiar", None);
        let mut other = base.clone();
        put(&mut base, actor.opponent(), "Valkyrie's Call", Zone::Hand);
        put(
            &mut other,
            actor.opponent(),
            "Perforating Artist",
            Zone::Hand,
        );
        let one = own(base);
        let two = own(other);
        assert_eq!(
            one.decision_v61().unwrap().candidates_v61(),
            two.decision_v61().unwrap().candidates_v61()
        );
        for name in [
            "Valkyrie's Call",
            "Perforating Artist",
            "Niv-Mizzet, Visionary",
            "Banner of Kinship",
            "Sphinx's Tutelage",
            "Consuming Aberration",
        ] {
            let mut raw = native::ready(actor);
            put(&mut raw, actor, name, Zone::Battlefield);
            assert!(GraveyardReturnSessionV61::from_session(
                FastActorSessionV1::from_graveyard_return_v61_fixture_state(raw)
            )
            .is_err());
        }
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let familiar = card_def::card_id_by_name("Nine-Lives Familiar").unwrap();
        let mut deck = vec![forest; 40];
        deck[0] = familiar;
        assert!(GraveyardReturnSessionV61::reset_explicit_mainboards_v61(
            1,
            17,
            (100, 100),
            ["p0".into(), "p1".into()],
            [deck, vec![forest; 40]],
            actor
        )
        .is_err());
    }
}
#[test]
fn genuine_paid_copy_and_suppression_keep_revival_roster_without_inventing_death_ability_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        let copy = put(&mut state, actor, "Self-Reflection", Zone::Hand);
        let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        let mut owner = own(state);
        cast(&mut owner, familiar, None, 3);
        drain(&mut owner);
        cast(&mut owner, copy, Some(familiar), 6);
        drain(&mut owner);
        assert!(world(&owner).objects.iter().any(|(id, o)| id != familiar
            && o.zone == Zone::Battlefield
            && o.v4.is_token
            && o.counters.revival == 0
            && card_def::CARD_DEFS[usize::from(o.card_def)].name == "Nine-Lives Familiar"));
        cast(&mut owner, witness, Some(familiar), 1);
        drain(&mut owner);
        assert!(
            !crate::continuous_characteristics_v1::printed_abilities_active(
                world(&owner),
                familiar
            )
        );
        assert_eq!(world(&owner).objects.get(familiar).counters.revival, 8);
        check_restore(&mut owner);
        cast(&mut owner, abrade, Some(familiar), 2);
        drain(&mut owner);
        assert_eq!(world(&owner).objects.get(familiar).zone, Zone::Graveyard);
        assert!(owner
            .decision_v61()
            .unwrap()
            .observation()
            .graveyard_return
            .nine_lives_schedules
            .is_empty());
    }
}

#[test]
fn genuine_paid_season_control_change_return_uses_captured_controller_and_physical_owner_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = native::ready(actor);
        let season = put(&mut raw, actor, "Doubling Season", Zone::Hand);
        let familiar = put(&mut raw, actor, "Nine-Lives Familiar", Zone::Hand);
        let employment = put(
            &mut raw,
            actor.opponent(),
            "Involuntary Employment",
            Zone::Hand,
        );
        let abrade = put(&mut raw, actor, "Abrade", Zone::Hand);
        let mut owner = own(raw);
        cast(&mut owner, season, None, 5);
        drain(&mut owner);
        cast(&mut owner, familiar, None, 3);
        drain(&mut owner);
        assert_eq!(world(&owner).objects.get(familiar).counters.revival, 16);
        for _ in 0..250 {
            if owner
                .decision_v61()
                .unwrap()
                .actions()
                .iter()
                .any(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==employment.0))
            {
                break;
            }
            quiet(&mut owner);
        }
        assert_eq!(world(&owner).active_player, actor.opponent());
        assert!(matches!(
            world(&owner).step,
            crate::state::Step::Main1 | crate::state::Step::Main2
        ));
        cast(&mut owner, employment, Some(familiar), 4);
        drain(&mut owner);
        assert_eq!(
            world(&owner).objects.get(familiar).controller,
            actor.opponent()
        );
        cast(&mut owner, abrade, Some(familiar), 2);
        drain(&mut owner);
        let d = owner.decision_v61().unwrap();
        let schedule = &d.observation().graveyard_return.nine_lives_schedules[0];
        assert_eq!(schedule.source.captured_controller, actor.opponent().into());
        assert_eq!(schedule.source.original_source.owner, actor.into());
        assert_eq!(schedule.departure_revival, 16);
        assert_eq!(schedule.return_base_before_replacements, 15);
        check_restore(&mut owner);
        for _ in 0..250 {
            if world(&owner).objects.get(familiar).zone == Zone::Battlefield {
                break;
            }
            quiet(&mut owner);
        }
        assert_eq!(world(&owner).objects.get(familiar).owner, actor);
        assert_eq!(
            world(&owner).objects.get(familiar).controller,
            actor.opponent()
        );
        assert_eq!(world(&owner).objects.get(familiar).counters.revival, 15);
    }
}
#[test]
fn genuine_paid_ordinary_elf_mana_activation_keeps_original_action_and_native_counts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut raw = native::ready(actor);
        let elf = put(&mut raw, actor, "Elvish Mystic", Zone::Hand);
        let mut owner = own(raw);
        cast(&mut owner, elf, None, 1);
        drain(&mut owner);
        for _ in 0..300 {
            if owner
                .decision_v61()
                .unwrap()
                .actions()
                .iter()
                .any(|a| matches!(a,A::ActivateManaAbility{source,..}if source.arena_id==elf.0))
            {
                break;
            }
            quiet(&mut owner);
        }
        assert!(!world(&owner).objects.get(elf).summoning_sick);
        let before = world(&owner).players[actor.index()]
            .mana_pool
            .iter()
            .map(|v| u32::from(*v))
            .sum::<u32>();
        act(
            &mut owner,
            |a| matches!(a,A::ActivateManaAbility{source,..}if source.arena_id==elf.0),
        );
        assert!(world(&owner).objects.get(elf).tapped);
        let after = world(&owner).players[actor.index()]
            .mana_pool
            .iter()
            .map(|v| u32::from(*v))
            .sum::<u32>();
        assert_eq!(after, before + 1);
        check_restore(&mut owner);
    }
}
