use super::*;
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{advance_to_next_main_for, put};
use crate::ravenous_public_v51::OutcomeV51;
use crate::state::Zone;

fn expected(session: &HumanRavenousSessionV51) -> FastActorDecisionV1 {
    match session.current_response() {
        FastActorResponseV1::Decision(value) => value,
        other => panic!("actual Human V51 decision: {other:?}"),
    }
}
fn from_state(state: crate::state::GameState, actor: PlayerId) -> HumanRavenousSessionV51 {
    HumanRavenousSessionV51::from_session(
        FastActorSessionV1::from_ravenous_v51_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn fixture(actor: PlayerId, hidden: &str) -> (HumanRavenousSessionV51, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    for player in [actor, actor.opponent()] {
        for _ in 0..12 {
            put(&mut state, player, "Forest", Zone::Library);
        }
    }
    put(&mut state, actor, "Island", Zone::Hand);
    let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Hand);
    put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
    for _ in 0..2 {
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    }
    for _ in 0..7 {
        put(&mut state, actor, "Forest", Zone::Battlefield);
    }
    for _ in 0..2 {
        put(&mut state, actor, "Swamp", Zone::Battlefield);
    }
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    put(&mut state, actor.opponent(), "Mountain", Zone::Battlefield);
    (from_state(state, actor), amulet)
}
fn menu(
    session: &HumanRavenousSessionV51,
    projector: &mut HumanRavenousProjectorV51,
) -> HumanDecisionRavenousV51 {
    projector
        .project_current(session, expected(session))
        .unwrap()
}
fn submit(
    session: &mut HumanRavenousSessionV51,
    projector: &mut HumanRavenousProjectorV51,
    choose: impl Fn(&str) -> bool,
) -> HumanActionRequestV1 {
    let current = menu(session, projector);
    let row = current
        .actions
        .iter()
        .find(|row| choose(&row.label))
        .unwrap_or_else(|| panic!("actual Human V51 action absent: {:?}", current.actions));
    let request = HumanActionRequestV1 {
        prompt_seq: current.prompt_seq,
        action_index: row.action_index,
    };
    projector.submit(session, request.clone()).unwrap();
    request
}
fn drain(
    session: &mut HumanRavenousSessionV51,
    projector: &mut HumanRavenousProjectorV51,
) -> HumanDecisionRavenousV51 {
    for _ in 0..48 {
        let e = expected(session);
        if e.acting_player != session.human_seat {
            session
                .step_opponent_with(e, |_, actions| {
                    actions
                        .iter()
                        .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                        .and_then(|index| u32::try_from(index).ok())
                        .ok_or_else(|| {
                            crate::rl::RlContractError("actual opponent pass absent".into())
                        })
                })
                .unwrap();
        } else if session.inner.ravenous_fixture_state().stack.is_empty()
            && session
                .inner
                .ravenous_fixture_state()
                .engine
                .pending_triggers
                .is_empty()
        {
            return menu(session, projector);
        } else {
            submit(session, projector, |label| label == "Pass priority");
        }
    }
    panic!("actual Human V51 resolution did not finish");
}
fn gameplay(session: &HumanRavenousSessionV51) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.ravenous_human_fixture_state()).unwrap(),
        session.inner.policy_step_count(),
        session.inner.physical_decision_count(),
    )
}
fn mana(
    session: &mut HumanRavenousSessionV51,
    projector: &mut HumanRavenousProjectorV51,
    land: &str,
    color: &str,
) {
    submit(session, projector, |label| {
        label.contains(land) && label.contains(color)
    });
}
fn cast(session: &mut HumanRavenousSessionV51, projector: &mut HumanRavenousProjectorV51) {
    for _ in 0..2 {
        mana(session, projector, "Forest", "{G}");
    }
    submit(session, projector, |label| {
        label.starts_with("Begin casting Ravenous Amulet")
    });
    drain(session, projector);
}
fn draw_cost(session: &mut HumanRavenousSessionV51, projector: &mut HumanRavenousProjectorV51) {
    mana(session, projector, "Forest", "{G}");
    submit(session, projector, |label| {
        label.starts_with("Activate Ravenous Amulet") && label.contains("draw one card, then")
    });
}
fn pay_creature(session: &mut HumanRavenousSessionV51, projector: &mut HumanRavenousProjectorV51) {
    submit(session, projector, |label| {
        label.starts_with("Sacrifice Llanowar Elves")
            && label.contains(" to pay for Ravenous Amulet ")
    });
}
fn assert_safe(value: &HumanDecisionRavenousV51) {
    let json = serde_json::to_value(value).unwrap();
    let text = json.to_string();
    for private in [
        "arena_id",
        "card_db_id",
        "zone_change_count",
        "original_world",
        "output_world",
        "event_history",
        "tap_history_index",
        "input_history_index",
        "payment_history_start",
        "inline_effect",
        "engine_indexes",
        "ravenous_paid",
    ] {
        assert!(!text.contains(private), "private Human field {private}");
    }
    for cards in json["state"]["public"]["battlefield"].as_array().unwrap() {
        for card in cards.as_array().unwrap() {
            for field in ["soul", "plus_one_counters", "marked_damage"] {
                assert!(card[field].is_number(), "mandatory exact {field}");
            }
        }
    }
    for row in json["state"]["extensions"]["ravenous"]["activations"]
        .as_array()
        .unwrap()
    {
        assert!(row["sacrificed"].is_string());
        assert!(row["outcome"]["Draw"].get("card").is_none());
    }
}

#[test]
fn actual_human_paid_cast_draw_loss_typed_and_generic_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, amulet) = fixture(actor, "Lightning Bolt");
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        cast(&mut session, &mut projector);
        assert_eq!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(amulet)
                .zone,
            Zone::Battlefield
        );
        assert_eq!(
            session.inner.ravenous_fixture_state().players[actor.index()].mana_pool
                [ManaColor::G.pool_index()],
            0
        );
        let zero = menu(&session, &mut projector);
        assert_eq!(
            zero.state.public.battlefield[actor.index()]
                .iter()
                .find(|row| row.card.stable.name == "Ravenous Amulet")
                .unwrap()
                .soul,
            0
        );
        assert_safe(&zero);
        let meaning = &zero.state.extensions.ravenous.meanings[0];
        use crate::rules_meaning_ravenous_amulet_v51::{CostV51, InstructionV51};
        assert_eq!(
            meaning.ordered_costs,
            vec![
                CostV51::GenericMana(1),
                CostV51::TapSource,
                CostV51::SacrificeControlledCreature
            ]
        );
        assert_eq!(
            meaning.ordered_instructions,
            vec![
                InstructionV51::DrawOneForAbilityController,
                InstructionV51::PlaceOneSoulCounterOnOriginalBattlefieldSource
            ]
        );
        assert!(meaning.sorcery_speed_only && meaning.placement_uses_resolution_world);
        let loss = &zero.state.extensions.ravenous.meanings[1];
        assert!(
            loss.loss_uses_original_generation_lki && loss.untargeted && !loss.sorcery_speed_only
        );
        assert_eq!(
            crate::card_def::CARD_DEFS[usize::from(
                session
                    .inner
                    .ravenous_fixture_state()
                    .objects
                    .get(amulet)
                    .card_def
            )]
            .capability,
            crate::card_def::CardCapability::Full
        );
        draw_cost(&mut session, &mut projector);
        let cost_snapshot = session.snapshot_v51();
        let cost_menu = menu(&session, &mut projector);
        pay_creature(&mut session, &mut projector);
        let paid_snapshot = session.snapshot_v51();
        let generic = session.inner.snapshot_v1();
        let published = menu(&session, &mut projector);
        assert_eq!(
            published.state.extensions.ravenous.activations[0].outcome,
            OutcomeV51::Published
        );
        assert!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(amulet)
                .tapped
        );
        let finished = drain(&mut session, &mut projector);
        assert_eq!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(amulet)
                .counters
                .soul,
            1
        );
        assert!(
            matches!(finished.state.extensions.ravenous.activations[0].outcome, OutcomeV51::Draw { drew_card: true, ref placement } if placement.as_ref().is_some_and(|row| (row.before, row.added, row.after) == (0,1,1)))
        );
        assert_safe(&finished);
        for case in 0..5 {
            let current = menu(&session, &mut projector);
            let request = HumanActionRequestV1 {
                prompt_seq: current.prompt_seq,
                action_index: 0,
            };
            let public = &mut projector.pending.as_mut().unwrap().public;
            match case {
                0 => public.battlefield[actor.index()][0].marked_damage ^= 1,
                1 => public.battlefield[actor.index()][0].plus_one_counters ^= 1,
                2 => {
                    public.battlefield[actor.index()].pop();
                }
                3 => public.ravenous.activations[0].occurrence += 1,
                4 => public.fake.card_db_hash ^= 1,
                _ => unreachable!(),
            }
            public.projection_sha256 = crate::ravenous_observation_v51::commitment(public).unwrap();
            let before = gameplay(&session);
            assert_eq!(
                projector.submit(&mut session, request),
                Err(Error::StaleDecision)
            );
            assert_eq!(gameplay(&session), before);
        }
        let first = gameplay(&session);
        session.restore_v51(&paid_snapshot).unwrap();
        let restored = drain(&mut session, &mut projector);
        assert_eq!(restored.state, finished.state);
        assert_eq!(restored.actions, finished.actions);
        assert_eq!(gameplay(&session), first);
        session.inner.restore_v1(&generic);
        let restored = drain(&mut session, &mut projector);
        assert_eq!(restored.state, finished.state);
        assert_eq!(gameplay(&session), first);
        session.restore_v51(&cost_snapshot).unwrap();
        let restored_cost = menu(&session, &mut projector);
        assert_eq!(restored_cost.state, cost_menu.state);
        assert_eq!(restored_cost.actions, cost_menu.actions);
        pay_creature(&mut session, &mut projector);
        assert_eq!(drain(&mut session, &mut projector).state, finished.state);
        assert_eq!(gameplay(&session), first);

        // Preserve the genuinely paid journals. Ordinary engine turns supply
        // untap, then a fresh owning backend publishes that actual state.
        let mut state = session.inner.ravenous_fixture_state().clone();
        advance_to_next_main_for(&mut state, actor);
        let mut session = from_state(state, actor);
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        for _ in 0..4 {
            mana(&mut session, &mut projector, "Forest", "{G}");
        }
        let before = session.inner.ravenous_fixture_state().players[actor.opponent().index()].life;
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Activate Ravenous Amulet")
                && label.contains("soul counters it had before being sacrificed")
        });
        let snapshot = session.snapshot_v51();
        let final_view = drain(&mut session, &mut projector);
        assert_eq!(
            session.inner.ravenous_fixture_state().players[actor.opponent().index()].life,
            before - 1
        );
        let activation = final_view
            .state
            .extensions
            .ravenous
            .activations
            .last()
            .unwrap();
        assert_eq!(activation.self_sacrifice_soul_count, Some(1));
        assert!(activation.current_original_source.is_none());
        assert_eq!(
            activation.outcome,
            OutcomeV51::Loss {
                opponent: actor.opponent().into(),
                amount: 1
            }
        );
        assert_safe(&final_view);
        let first = gameplay(&session);
        session.restore_v51(&snapshot).unwrap();
        assert_eq!(drain(&mut session, &mut projector).state, final_view.state);
        assert_eq!(gameplay(&session), first);
    }
}

#[test]
fn human_v51_stale_forged_retry_foreign_owner_and_predecessor_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, _) = fixture(actor, "Lightning Bolt");
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        let visible = menu(&session, &mut projector);
        assert_eq!(menu(&session, &mut projector), visible);
        let request = HumanActionRequestV1 {
            prompt_seq: visible.prompt_seq,
            action_index: u32::MAX,
        };
        let before = gameplay(&session);
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::InvalidAction)
        );
        assert_eq!(gameplay(&session), before);
        let snapshot = session.snapshot_v51();
        let generic = session.inner.snapshot_v1();
        let request = submit(&mut session, &mut projector, |label| {
            label.contains("Forest") && label.contains("{G}")
        });
        let after = gameplay(&session);
        assert!(projector.submit(&mut session, request.clone()).is_ok());
        assert_eq!(gameplay(&session), after);
        assert_eq!(
            projector.submit(
                &mut session,
                HumanActionRequestV1 {
                    action_index: request.action_index ^ 1,
                    ..request.clone()
                }
            ),
            Err(Error::ConflictingRetry)
        );
        let (mut foreign, _) = fixture(actor, "Lightning Bolt");
        assert_eq!(
            projector.submit(&mut foreign, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(foreign.restore_v51(&snapshot), Err(Error::StaleDecision));
        session.restore_v51(&snapshot).unwrap();
        assert_eq!(
            projector.submit(&mut session, request.clone()),
            Err(Error::StaleDecision)
        );
        menu(&session, &mut projector);
        let request = HumanActionRequestV1 {
            prompt_seq: projector.pending.as_ref().unwrap().visible.prompt_seq,
            action_index: 0,
        };
        session.inner.restore_v1(&generic);
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        menu(&session, &mut projector);
        let request = HumanActionRequestV1 {
            prompt_seq: projector.pending.as_ref().unwrap().visible.prompt_seq,
            action_index: 0,
        };
        projector.pending.as_mut().unwrap().public.battlefield[actor.index()][0].soul ^= 1;
        let before = gameplay(&session);
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(gameplay(&session), before);
        assert!(super::super::fake_v50::HumanFakeSessionV50::from_session(
            session.inner.clone(),
            actor.into()
        )
        .is_err());
        assert!(serde_json::from_str::<HumanActionRequestV1>(
            "{\"prompt_seq\":1,\"action_index\":0,\"engine_index\":3}"
        )
        .is_err());
    }
}

#[test]
fn human_v51_paid_fake_coexistence_return_and_exact_wide_values_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut session, amulet) = fixture(actor, "Lightning Bolt");
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        for _ in 0..2 {
            mana(&mut session, &mut projector, "Swamp", "{B}");
        }
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Begin casting Fake Your Own Death")
        });
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Fake Your Own Death [")
                && label.contains(": target Llanowar Elves [")
        });
        let fake = drain(&mut session, &mut projector);
        let target = fake.state.extensions.fake.grants[0]
            .original_recipient
            .handle
            .clone();
        cast(&mut session, &mut projector);
        draw_cost(&mut session, &mut projector);
        let current = menu(&session, &mut projector);
        let name = current.state.extensions.fake.grants[0]
            .original_recipient
            .handle
            .clone();
        assert!(!target.is_empty());
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Sacrifice Llanowar Elves")
                && label.contains(" to pay for Ravenous Amulet ")
                && label.contains(&format!("[{name}]"))
        });
        let snapshot = session.snapshot_v51();
        let finished = drain(&mut session, &mut projector);
        assert_eq!(finished.state.extensions.fake.deaths.len(), 1);
        assert!(finished.state.extensions.fake.deaths[0]
            .returned_incarnation
            .is_some());
        assert_eq!(finished.state.extensions.fake.deaths[0].treasures.len(), 1);
        assert_eq!(finished.state.extensions.ravenous.activations.len(), 1);
        assert_safe(&finished);
        let first = gameplay(&session);
        session.restore_v51(&snapshot).unwrap();
        assert_eq!(drain(&mut session, &mut projector).state, finished.state);
        assert_eq!(gameplay(&session), first);

        // Numeric fixture operands on the paid public artifact, not an invented
        // activation owner or an additional printed counter producer.
        let mut state = session.inner.ravenous_fixture_state().clone();
        state.objects.get_mut(amulet).counters.plus1_plus1 = i32::from(i16::MAX) + 17;
        state.objects.get_mut(amulet).counters.soul = i32::from(i16::MAX) + 29;
        state.objects.get_mut(amulet).damage = u32::from(u16::MAX) + 23;
        let mut variant = state.clone();
        let hidden = variant.players[actor.opponent().index()].hand[0];
        variant.objects.get_mut(hidden).card_def =
            crate::card_def::card_id_by_name("Mountain").unwrap();
        let first = from_state(state.clone(), actor);
        let second = from_state(variant, actor);
        let mut left = HumanRavenousProjectorV51::new(actor.into());
        let mut right = HumanRavenousProjectorV51::new(actor.into());
        let view = menu(&first, &mut left);
        assert_eq!(view, menu(&second, &mut right));
        let row = view.state.public.battlefield[actor.index()]
            .iter()
            .find(|row| row.card.stable.name == "Ravenous Amulet")
            .unwrap();
        assert_eq!(
            (row.soul, row.plus_one_counters, row.marked_damage),
            (
                i32::from(i16::MAX) + 29,
                i32::from(i16::MAX) + 17,
                u32::from(u16::MAX) + 23
            )
        );
        for seat in 0..2 {
            assert_eq!(
                view.state.public.battlefield[seat].len(),
                state.players[seat].battlefield.len()
            );
        }
        assert_safe(&view);
    }
}

#[test]
fn actual_paid_human_sacrifice_facts_omit_underlying_face_down_printing_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        // Reuse the existing helper's genuinely paid engine cast. Only the
        // physical face-down creature and mana land are fixture operands.
        let (mut state, amulet) =
            crate::event::ravenous_amulet_paid_future_tests::cast_amulet(actor);
        let chosen = put(&mut state, actor, "Mountain", Zone::Battlefield);
        state.objects.get_mut(chosen).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: true,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Forest", Zone::Battlefield);
        let mut session = from_state(state, actor);
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        draw_cost(&mut session, &mut projector);
        let cost = menu(&session, &mut projector);
        assert!(cost
            .actions
            .iter()
            .any(|row| row.label.starts_with("Sacrifice Face-down creature")));
        assert!(!cost
            .actions
            .iter()
            .any(|row| row.label.starts_with("Sacrifice Mountain")));
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Sacrifice Face-down creature")
        });
        let snapshot = session.snapshot_v51();
        let finished = drain(&mut session, &mut projector);
        assert_eq!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(chosen)
                .zone,
            Zone::Graveyard
        );
        assert_eq!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(amulet)
                .counters
                .soul,
            1
        );
        let facts = serde_json::to_value(&finished.state.extensions.ravenous).unwrap();
        let text = facts.to_string();
        assert!(!text.contains("Mountain"));
        assert!(!text.contains("Forest"));
        assert!(facts["activations"][0]["sacrificed"]
            .as_str()
            .unwrap()
            .starts_with("ravenous_incarnation"));
        assert_safe(&finished);
        session.restore_v51(&snapshot).unwrap();
        assert_eq!(drain(&mut session, &mut projector).state, finished.state);
    }
}

#[test]
fn paid_pole_and_longbow_grants_on_paid_animated_amulet_keep_their_own_labels_and_costs_both_seats()
{
    use crate::effect::ExecCtx;
    use crate::engine::{self, Action, Decision};
    use crate::event::ravenous_amulet_paid_future_tests::{cast_amulet, finish};
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, amulet) = cast_amulet(actor);
        advance_to_next_main_for(&mut state, actor);
        // A captured animation is a primitive operand. The Amulet, Equipment
        // casts, equip payments and granted activation use actual engine paths.
        let animator = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let mut context = ExecCtx::no_targets(animator, actor);
        context.ability_source_contract = Some(crate::state::AbilitySourceContractV4::capture(
            &state, animator,
        ));
        context.targets.push(Target::Object(amulet));
        context
            .target_contracts
            .push(crate::state::StackTargetContractV4::capture(
                &state,
                Target::Object(amulet),
            ));
        crate::standard_creatures_v1::execute(
            crate::standard_creatures_v1::CreatureEffectV1::ToughCookieAnimate,
            &context,
            &mut state,
        );
        assert!(engine::object_has_type(
            &state,
            amulet,
            crate::card_def::CardType::Creature
        ));
        let mut poles = Vec::new();
        for (name, equip_cost) in [("Fishing Pole", 2), ("Viridian Longbow", 3)] {
            let equipment = put(&mut state, actor, name, Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(equipment)).unwrap();
            finish(&mut state);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = equip_cost;
            engine::step(&mut state, Action::ActivateAbility(equipment, 0)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(amulet))).unwrap();
            finish(&mut state);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            assert_eq!(
                state.objects.get(equipment).v4.attached_to.unwrap().object,
                amulet
            );
            poles.push(equipment);
        }
        let pole = poles[0];
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        let mut session = from_state(state, actor);
        let mut projector = HumanRavenousProjectorV51::new(actor.into());
        let view = menu(&session, &mut projector);
        assert!(view.actions.iter().any(|row| row
            .label
            .starts_with("Activate Fishing Pole-granted ability")));
        assert!(view
            .actions
            .iter()
            .any(|row| row.label.starts_with("Activate granted ability")
                && row.label.contains("1 damage")));
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Activate Fishing Pole-granted ability")
        });
        let cost = menu(&session, &mut projector);
        assert!(cost
            .actions
            .iter()
            .any(|row| row.label.starts_with("Tap Fishing Pole")));
        assert!(!cost
            .actions
            .iter()
            .any(|row| row.label.starts_with("Sacrifice ")));
        let snapshot = session.snapshot_v51();
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Tap Fishing Pole")
        });
        let public = drain(&mut session, &mut projector);
        assert!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(pole)
                .tapped
        );
        assert_eq!(
            session
                .inner
                .ravenous_fixture_state()
                .objects
                .get(pole)
                .counters
                .bait,
            1
        );
        assert!(public.state.extensions.ravenous.activations.is_empty());
        assert_safe(&public);
        let first = gameplay(&session);
        session.restore_v51(&snapshot).unwrap();
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Tap Fishing Pole")
        });
        assert_eq!(drain(&mut session, &mut projector).state, public.state);
        assert_eq!(gameplay(&session), first);
    }
}
