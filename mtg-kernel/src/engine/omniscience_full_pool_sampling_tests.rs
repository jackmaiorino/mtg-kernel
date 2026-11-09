use crate::{
    card_def::{card_id_by_name, CARD_DEFS},
    current_mixed_policy_v1::{KaitoDecision, KaitoSession},
    effect::thousand_year_storm_v1_tests as native,
    engine::{Action, Decision},
    event::{self, CommittedEvent, ProposedEvent},
    ids::{ObjectId, PlayerId},
    limited_match_v1::LimitedBeliefV1,
    omniscience_consumer_v1::ActionV1 as OmniAction,
    omniscience_public_v1::RouteV1,
    policy_observation_v6::tests::put,
    policy_surface_v5::LimitedAppendixV1,
    rl::{ActionSemanticV1 as VisibleAction, TargetRefV1},
    rl_session::{FastActorResponseV1, FastActorSessionV1},
    state::{GameState, ObjectStateV4, Target, Zone},
};

fn setup(actor: PlayerId) -> GameState {
    let mut state = native::ready(actor, "Forest");
    FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
    state.reveal_knowledge_v4 = Some(Default::default());
    state
}

fn raw(state: &GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state.clone(), LimitedAppendixV1::FullPool,
    )
}

fn owner(state: &GameState) -> KaitoSession {
    KaitoSession::from_session(raw(state)).unwrap()
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("historical source fixture lost current menu: {:?}", owner.current_response());
    };
    owner.encode(expected).unwrap()
}

fn priority(state: &mut GameState, viewer: PlayerId) {
    for _ in 0..8 {
        match native::next(state) {
            Decision::CastSpellOrPass { player, .. } if player == viewer => return,
            Decision::CastSpellOrPass { .. } => native::action(state, Action::Pass),
            other => panic!("expected actual native priority: {other:?}"),
        }
    }
    panic!("viewer did not get native priority");
}

fn corpus() -> LimitedBeliefV1 {
    // Public corpus with no information about any actual hand slot.
    LimitedBeliefV1::new([vec![card_id_by_name("Forest").unwrap(); 200]])
}

fn alternate_hidden(state: &GameState, id: ObjectId) -> GameState {
    let mut alternate = state.clone();
    let hidden = alternate.objects.get_mut(id);
    assert_eq!(hidden.zone, Zone::Hand);
    hidden.card_def = card_id_by_name("Mountain").unwrap();
    hidden.name = CARD_DEFS[usize::from(hidden.card_def)].name.into();
    hidden.v4 = ObjectStateV4::from_card_def(hidden.card_def);
    alternate
}

fn pass_with_native_parity(owner: &mut KaitoSession, current: &KaitoDecision) {
    let binding = current.fixture_backend_v75();
    let selected = binding.candidates_v75().candidates.iter()
        .position(|row| matches!(row.action, VisibleAction::Pass { .. })).unwrap() as u32;
    let original = binding.fixture_original_index_v75(selected).unwrap();
    let expected = binding.expected();
    let mut native = raw(owner.fixture_state());
    let result = native.step(expected.episode_id, expected.step, original).unwrap();
    assert_eq!(owner.apply(current, selected).unwrap(), result);
    assert_eq!(owner.fixture_state(), native.kaito_fixture_state());
}

#[test]
fn paid_omniscience_keeps_a_finalized_free_spell_after_hidden_permission_rebinding_both_seats() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let viewer = caster.opponent();
        let mut scene = setup(caster);
        let permission = super::tests::paid(&mut scene, caster);
        let child = put(&mut scene, caster, "Burst Lightning", Zone::Hand);
        let mut numeric = owner(&scene);
        let tapped = |state: &GameState| state.players[caster.index()].battlefield.iter()
            .filter(|&&id| state.objects.get(id).tapped).count();
        let before = tapped(numeric.fixture_state());
        let mut finalized = false;
        for _ in 0..10 {
            if numeric.fixture_state().objects.get(child).zone == Zone::Stack
                && numeric.fixture_state().engine.pending_cast.is_none()
            {
                assert_eq!(tapped(numeric.fixture_state()), before);
                finalized = true;
                break;
            }
            let d = decision(&numeric);
            let rows = &d.fixture_backend_v75().candidates_v75().candidates;
            let selected = rows.iter().position(|row| {
                row.omniscience_action == Some(OmniAction::ChooseRoute {
                    route: RouteV1::ReplaceBaseMana,
                }) || match &row.action {
                    VisibleAction::CastSpell { source, .. } => source.arena_id == child.0,
                    VisibleAction::ChooseKicker { pay, .. } => !pay,
                    VisibleAction::ChooseTarget { target: TargetRefV1::Player { player }, .. } =>
                        *player == viewer.into(),
                    _ => false,
                }
            }).expect("genuine Omniscience cast continuation");
            numeric.apply(&d, selected as u32).unwrap();
        }
        assert!(finalized);

        let mut state = numeric.fixture_state().clone();
        // Explicit fixture zone primitive after the paid permanent and real
        // finalized cast. The normal public transfer reveals the name in hand.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(permission, Zone::Hand));
        let hidden = put(&mut state, caster, "Island", Zone::Hand);
        put(&mut state, caster, "Forest", Zone::Hand);
        priority(&mut state, viewer);
        let paired = alternate_hidden(&state, hidden);
        let live = owner(&state);
        let other = owner(&paired);
        let d = decision(&live);
        let other_d = decision(&other);
        assert_eq!(d.packet(), other_d.packet());
        let untouched = live.fixture_state().clone();
        let before_life = state.players[viewer.index()].life;
        let mut rebound = false;
        for seed in [552_478, 552_479, 552_480, 552_481] {
            let mut sampled = live.sampled_full_pool_closed_list_v1(&d, seed, &corpus()).unwrap();
            let alternate = other.sampled_full_pool_closed_list_v1(&other_d, seed, &corpus()).unwrap();
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), d.packet());
            assert_eq!(fresh.packet(), decision(&alternate).packet());
            let facts = fresh.fixture_backend_v75().observation().omniscience.as_ref().unwrap();
            let cast = facts.casts.iter().find(|row| row.spell.object == child.0).unwrap();
            assert_eq!(cast.route, Some(RouteV1::ReplaceBaseMana));
            assert_eq!(cast.permission.original_source.object, permission.0);
            assert!(cast.permission.current_original_source.is_none());
            assert!(!cast.original_permission_active);
            rebound |= sampled.fixture_state().objects.get(permission).card_def
                != crate::omniscience_public_v1::PRINTING;

            let saved = sampled.snapshot();
            pass_with_native_parity(&mut sampled, &fresh);
            assert_eq!(sampled.fixture_state().objects.get(child).zone, Zone::Graveyard);
            assert_eq!(sampled.fixture_state().players[viewer.index()].life, before_life - 2);
            sampled.restore(&saved).unwrap();
            assert_eq!(decision(&sampled).packet(), d.packet());
            assert!(sampled.apply(&fresh, 0).is_err());
        }
        assert!(rebound);
        assert_eq!(live.fixture_state(), &untouched);
    }
}

#[test]
fn paid_electroduplicate_keeps_original_stale_target_after_sampled_bounce_both_seats() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        let viewer = caster.opponent();
        let mut state = setup(caster);
        let creature = native::cast(&mut state, caster, "Llanowar Elves", &[], 1);
        native::finish(&mut state);
        let spell = native::cast(
            &mut state, caster, "Electroduplicate", &[Target::Object(creature)], 3,
        );
        let companion = put(&mut state, viewer, "Llanowar Elves", Zone::Battlefield);
        let bounce = native::cast(
            &mut state, viewer, "Run Away Together",
            &[Target::Object(creature), Target::Object(companion)], 2,
        );
        for _ in 0..8 {
            if state.objects.get(bounce).zone == Zone::Graveyard {
                break;
            }
            assert!(matches!(native::next(&mut state), Decision::CastSpellOrPass { .. }));
            native::action(&mut state, Action::Pass);
        }
        assert_eq!(state.objects.get(bounce).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(creature).zone, Zone::Hand);
        assert_eq!(state.objects.get(spell).zone, Zone::Stack);
        let hidden = put(&mut state, caster, "Island", Zone::Hand);
        put(&mut state, caster, "Forest", Zone::Hand);
        priority(&mut state, viewer);
        let pair = alternate_hidden(&state, hidden);
        let live = owner(&state);
        let paired = owner(&pair);
        let root = decision(&live);
        let paired_root = decision(&paired);
        assert_eq!(root.packet(), paired_root.packet());
        let untouched = live.fixture_state().clone();
        let mut rebound = false;
        for seed in [552_471, 552_472, 552_473, 552_474] {
            let mut sampled = live.sampled_full_pool_closed_list_v1(&root, seed, &corpus()).unwrap();
            let other = paired.sampled_full_pool_closed_list_v1(&paired_root, seed, &corpus()).unwrap();
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), root.packet());
            assert_eq!(fresh.packet(), decision(&other).packet());
            let facts = fresh.fixture_backend_v75().observation().electroduplicate.as_ref().unwrap();
            let cast = facts.casts.iter().find(|row| row.spell.object == spell.0).unwrap();
            let target = cast.target.as_ref().unwrap();
            assert_eq!(target.original.object, creature.0);
            assert!(target.current_original.is_none());
            assert!(!target.currently_legal);

            if sampled.fixture_state().objects.get(creature).card_def != card_id_by_name("Llanowar Elves").unwrap() {
                rebound = true;
                let mut missing = sampled.fixture_state().clone();
                let item = missing.stack.iter().find(|row| row.source == spell).unwrap().v4.stack_item_id;
                missing.engine.event_history.retain(|event| !matches!(
                    event, CommittedEvent::ThousandYearStormV1 {
                        journal: crate::effect::thousand_year_storm_v1::JournalV1::TargetFaces {
                            item: owner, ..
                        },
                    } if *owner == item
                ));
                assert!(crate::electroduplicate_public_v1::capture(&missing, viewer).is_err());
            }
            let saved = sampled.snapshot();
            pass_with_native_parity(&mut sampled, &fresh);
            assert_eq!(sampled.fixture_state().objects.get(spell).zone, Zone::Graveyard);
            assert!(decision(&sampled).fixture_backend_v75().observation()
                .electroduplicate.as_ref().unwrap().copied_tokens.is_empty());
            sampled.restore(&saved).unwrap();
            assert_eq!(decision(&sampled).packet(), root.packet());
        }
        assert!(rebound);
        assert_eq!(live.fixture_state(), &untouched);
    }
}

#[test]
fn paid_equipment_ability_keeps_its_stale_target_and_fizzles_after_hidden_sampling() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for equipment in ["Embercleave", "Leyline Axe"] {
            let viewer = caster.opponent();
            let mut state = setup(caster);
            let creature = native::cast(&mut state, caster, "Llanowar Elves", &[], 1);
            native::finish(&mut state);
            let source = native::cast(&mut state, caster, equipment, &[], if equipment == "Embercleave" { 6 } else { 4 });
            if equipment == "Leyline Axe" {
                native::finish(&mut state);
                priority(&mut state, caster);
                let lands = state.players[caster.index()].battlefield.iter().copied().filter(|&id| {
                    let object = state.objects.get(id);
                    !object.tapped && object.card_def == card_id_by_name("Forest").unwrap()
                }).take(3).collect::<Vec<_>>();
                assert_eq!(lands.len(), 3);
                for land in lands { native::action(&mut state, Action::ActivateManaAbility(land)); }
                native::action(&mut state, Action::ActivateAbility(source, 0));
            }
            let mut chose = false;
            for _ in 0..12 {
                match native::next(&mut state) {
                    Decision::ChooseTargets { spell, legal_targets, .. } if spell == source => {
                        assert!(legal_targets.contains(&Target::Object(creature)));
                        native::action(&mut state, Action::ChooseTarget(Target::Object(creature)));
                        chose = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => native::action(&mut state, Action::Pass),
                    other => panic!("equipment target was not offered: {other:?}"),
                }
            }
            assert!(chose);
            assert!(matches!(native::next(&mut state), Decision::CastSpellOrPass { .. }));
            let original = state.stack.iter().find(|row| row.source == source
                && row.kind != crate::state::StackItemKind::Spell).unwrap().clone();
            let companion = put(&mut state, viewer, "Llanowar Elves", Zone::Battlefield);
            let bounce = native::cast(&mut state, viewer, "Run Away Together",
                &[Target::Object(creature), Target::Object(companion)], 2);
            for _ in 0..8 {
                if state.objects.get(bounce).zone == Zone::Graveyard { break; }
                assert!(matches!(native::next(&mut state), Decision::CastSpellOrPass { .. }));
                native::action(&mut state, Action::Pass);
            }
            assert_eq!(state.objects.get(creature).zone, Zone::Hand);
            assert!(state.stack.contains(&original));
            let hidden = put(&mut state, caster, "Island", Zone::Hand);
            put(&mut state, caster, "Forest", Zone::Hand);
            priority(&mut state, viewer);
            let live = owner(&state);
            let paired = owner(&alternate_hidden(&state, hidden));
            let root = decision(&live);
            let other_root = decision(&paired);
            assert_eq!(root.packet(), other_root.packet());
            let mut rebound = false;
            for seed in [552_462, 552_463, 552_464, 552_465] {
                let mut sampled = live.sampled_full_pool_closed_list_v1(&root, seed, &corpus()).unwrap();
                let other = paired.sampled_full_pool_closed_list_v1(&other_root, seed, &corpus()).unwrap();
                let fresh = decision(&sampled);
                assert_eq!(fresh.packet(), root.packet());
                assert_eq!(fresh.packet(), decision(&other).packet());
                let contract = original.v4.target_contracts[0];
                assert!(!crate::engine::target_contract_matches_live(sampled.fixture_state(), Target::Object(creature), contract));
                if sampled.fixture_state().objects.get(creature).card_def != card_id_by_name("Llanowar Elves").unwrap() {
                    rebound = true;
                    let mut missing = sampled.fixture_state().clone();
                    missing.sampled_history_v1 = Default::default();
                    assert!(crate::engine::validated_stack_item_target_spec(&original, &missing).is_err());
                    let mut changed = original.clone();
                    changed.v4.stack_item_id.0 += 1;
                    assert!(crate::sampled_history_v1::object_target_face(sampled.fixture_state(), &changed, 0, contract).is_none());
                }
                let saved = sampled.snapshot();
                pass_with_native_parity(&mut sampled, &fresh);
                assert!(!sampled.fixture_state().stack.iter().any(|row| row.v4.stack_item_id == original.v4.stack_item_id));
                assert!(sampled.fixture_state().objects.get(source).v4.attached_to.is_none());
                sampled.restore(&saved).unwrap();
                assert_eq!(decision(&sampled).packet(), root.packet());
            }
            assert!(rebound, "sampled target never changed for {equipment}");
            assert_eq!(live.fixture_state(), &state);
        }
    }
}
