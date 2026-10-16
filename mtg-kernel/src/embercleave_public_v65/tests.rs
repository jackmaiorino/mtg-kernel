//! Paid source fixtures. Setup permanents and explicit state operands are labelled.
use super::*;
use crate::{
    card_def::{card_id_by_name, CardCapability, Keywords},
    engine::{
        advance_until_decision, checked_effective_pt, has_effective_keyword,
        normal_cast_reduction_count, step, Action, Decision,
    },
    event::{self, ProposedEvent},
    mana,
    policy_observation_v6::tests::put,
    state::Step,
};
fn effective_normal_cast_cost(
    d: &card_def::CardDef,
    actor: PlayerId,
    state: &GameState,
) -> mana::Cost {
    engine::printed_normal_cast_cost_with_targets(d, actor, &[], state)
}
fn bytes(state: &GameState) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}
fn assert_projection(state: &GameState) -> GameState {
    let before = bytes(state);
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        let actual = capture(state, viewer).unwrap();
        assert_eq!(capture(&restored, viewer).unwrap(), actual);
        validate(state, viewer, &actual).unwrap();
        assert_eq!(
            serde_json::from_slice::<FactsV65>(&serde_json::to_vec(&actual).unwrap()).unwrap(),
            actual
        );
        assert!(!actual.complete_ordinary_observation && !actual.owning_consumer_admitted);
    }
    assert_eq!(bytes(state), before);
    assert_eq!(bytes(&restored), before);
    restored
}
fn count(state: &GameState, player: PlayerId) -> u32 {
    let native = normal_cast_reduction_count(
        DynamicCountDef::ControllerAttackingCreatures,
        player,
        &[],
        state,
    );
    let facts = capture(state, player).unwrap();
    let actual = &facts.current_cast_reduction[player.index()];
    assert_eq!(actual.count, native);
    assert_eq!(
        actual.current_attackers.len(),
        usize::try_from(native).unwrap()
    );
    assert_eq!(
        actual.generic_due_now,
        4_u8.saturating_sub(u8::try_from(native.min(4)).unwrap())
    );
    assert_eq!(
        (actual.retained_red_pips, actual.printed_mana_value),
        (2, 6)
    );
    native
}
fn next(state: &mut GameState) -> Decision {
    let value = advance_until_decision(state);
    assert!(!matches!(value, Decision::Halted { .. }), "{value:?}");
    value
}
fn quiet(state: &mut GameState) {
    match next(state) {
        Decision::CastSpellOrPass { .. } => step(state, Action::Pass).unwrap(),
        Decision::DeclareBlockers { .. } => {
            step(state, Action::DeclareBlockers(Vec::new())).unwrap()
        }
        other => panic!("unexpected actual fixture choice: {other:?}"),
    }
}
fn settle(state: &mut GameState) {
    for _ in 0..32 {
        if matches!(next(state), Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(state);
    }
    panic!("native equipment stack did not settle");
}

#[test]
fn current_attacker_cost_count_tracks_native_declaration_departure_control_and_combat_end_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let second = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        state.step = Step::DeclareAttackers;
        assert_eq!(count(&state, actor), 0);
        assert!(
            matches!(next(&mut state), Decision::DeclareAttackers { player, .. } if player == actor)
        );
        step(&mut state, Action::DeclareAttackers(vec![first, second])).unwrap();
        assert_eq!(count(&state, actor), 2);
        assert_eq!(count(&state, actor.opponent()), 0);
        let declared = state.clone();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&declared).unwrap()).unwrap();
        assert_eq!(restored, declared);
        assert_eq!(count(&restored, actor), 2);

        // Real control transfer removes the old attacker from combat. It
        // cannot become an attacking creature of the new controller.
        crate::standard_cards_v1::gain_control(&mut state, second, actor.opponent());
        assert_eq!(count(&state, actor), 1);
        assert_eq!(count(&state, actor.opponent()), 0);
        // Labelled departure/reentry primitive: the same physical ID returns,
        // but its new battlefield incarnation has never attacked.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(first, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(first, Zone::Battlefield),
        );
        assert_eq!(count(&state, actor), 0);
        state = restored;
        for _ in 0..32 {
            if state.step == Step::Main2 {
                break;
            }
            quiet(&mut state);
        }
        assert_eq!(state.step, Step::Main2);
        assert_eq!(count(&state, actor), 0);
        assert!(state.engine.combat.attackers.is_empty());
    }
}

#[test]
fn genuine_paid_flash_attach_equip_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for attackers in [0_usize, 1, 4, 6] {
            let mut state = crate::extra_turn_v1::tests::ready(actor);
            let source = put(&mut state, actor, "Embercleave", Zone::Hand);
            let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let alternate = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let attacking = (0..attackers)
                .map(|_| put(&mut state, actor, "Llanowar Elves", Zone::Battlefield))
                .collect::<Vec<_>>();
            let lands = (0..9)
                .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
                .collect::<Vec<_>>();
            state.step = Step::DeclareAttackers;
            assert!(matches!(
                next(&mut state),
                Decision::DeclareAttackers { .. }
            ));
            step(&mut state, Action::DeclareAttackers(attacking)).unwrap();
            assert_eq!(count(&state, actor), attackers as u32);
            let def = &card_def::CARD_DEFS[usize::from(card_id_by_name("Embercleave").unwrap())];
            assert_eq!(card_id_by_name("Embercleave"), Some(462));
            assert_eq!(def.capability, card_def::CardCapability::Partial);
            assert_eq!(def.types, &[CardType::Artifact]);
            assert!(def.keywords.has(Keywords::FLASH));
            let cost = effective_normal_cast_cost(def, actor, &state);
            assert_eq!(cost.generic, 4_u8.saturating_sub(attackers as u8));
            assert_eq!(
                cost.pips,
                &[
                    mana::Pip::Colored(mana::ManaColor::R),
                    mana::Pip::Colored(mana::ManaColor::R)
                ]
            );
            let due = usize::from(cost.generic) + 2;
            for land in &lands[..due] {
                step(&mut state, Action::ActivateManaAbility(*land)).unwrap();
            }
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor && castable_spells.contains(&source))
            );
            step(&mut state, Action::CastSpell(source)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            assert!(state.engine.pending_cast.is_none());
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(lands[..due].iter().all(|id| state.objects.get(*id).tapped));
            assert_eq!(state.objects.get(source).zone, Zone::Stack);
            assert!(state
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
                crate::event::CommittedEvent::SpellCast { spell, controller }
                if *spell == source && *controller == actor)));
            for _ in 0..16 {
                if matches!(next(&mut state), Decision::ChooseTargets { spell, .. } if spell == source)
                {
                    break;
                }
                quiet(&mut state);
            }
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { player, spell, remaining: 1, legal_targets, can_finish: false }
                if player == actor && spell == source && legal_targets.contains(&Target::Object(host)))
            );
            assert_projection(&state);
            let waiting = state.clone();
            let mut expected = None;
            for mut replay in [
                waiting.clone(),
                serde_json::from_slice::<GameState>(&serde_json::to_vec(&waiting).unwrap())
                    .unwrap(),
            ] {
                step(&mut replay, Action::ChooseTarget(Target::Object(host))).unwrap();
                assert!(matches!(
                    next(&mut replay),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_projection(&replay);
                let published = replay.clone();
                settle(&mut replay);
                let link = replay.objects.get(source).v4.attached_to.unwrap();
                assert_eq!(link.object, host);
                assert_eq!(
                    link.zone_change_count,
                    replay.objects.get(host).zone_change_count
                );
                assert_eq!(checked_effective_pt(&replay, host).unwrap(), (2, 2));
                for keyword in [Keywords::DOUBLE_STRIKE, Keywords::TRAMPLE] {
                    assert!(has_effective_keyword(&replay, host, keyword));
                }
                let mut published_restore: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&published).unwrap()).unwrap();
                settle(&mut published_restore);
                assert_eq!(published_restore, replay);
                if let Some(previous) = &expected {
                    assert_eq!(&replay, previous);
                } else {
                    expected = Some(replay);
                }
            }
            // A departed source cannot attach from the controller's hand.
            // These are labelled zone/control event primitives on the
            // genuinely paid history, not claims of paid response spells.
            let mut replaced = waiting.clone();
            step(&mut replaced, Action::ChooseTarget(Target::Object(host))).unwrap();
            assert!(matches!(
                next(&mut replaced),
                Decision::CastSpellOrPass { .. }
            ));
            event::propose_and_commit(
                &mut replaced,
                ProposedEvent::zone_change(source, Zone::Hand),
            );
            settle(&mut replaced);
            assert!(replaced.objects.get(source).v4.attached_to.is_none());
            assert_eq!(replaced.objects.get(source).zone, Zone::Hand);
            // Returning that same physical Equipment creates a new source
            // incarnation; the published old trigger cannot attach it.
            let mut reentered = waiting.clone();
            step(&mut reentered, Action::ChooseTarget(Target::Object(host))).unwrap();
            assert!(matches!(
                next(&mut reentered),
                Decision::CastSpellOrPass { .. }
            ));
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(source, Zone::Hand),
            );
            event::propose_and_commit(
                &mut reentered,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            // The reentry's own mandatory ETB must be answered independently.
            assert!(
                matches!(next(&mut reentered), Decision::ChooseTargets { spell, .. } if spell == source)
            );
            step(
                &mut reentered,
                Action::ChooseTarget(Target::Object(alternate)),
            )
            .unwrap();
            next(&mut reentered);
            settle(&mut reentered);
            assert_eq!(
                reentered.objects.get(source).v4.attached_to.unwrap().object,
                alternate
            );
            for mode in 0..2 {
                let mut changed = waiting.clone();
                step(&mut changed, Action::ChooseTarget(Target::Object(host))).unwrap();
                assert!(matches!(
                    next(&mut changed),
                    Decision::CastSpellOrPass { .. }
                ));
                if mode == 0 {
                    event::propose_and_commit(
                        &mut changed,
                        ProposedEvent::zone_change(host, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut changed,
                        ProposedEvent::zone_change(host, Zone::Battlefield),
                    );
                } else {
                    crate::standard_cards_v1::gain_control(&mut changed, host, actor.opponent());
                }
                settle(&mut changed);
                assert!(changed.objects.get(source).v4.attached_to.is_none());
            }

            state = expected.unwrap();
            for _ in 0..64 {
                if state.step == Step::Main2 {
                    break;
                }
                quiet(&mut state);
            }
            assert_eq!(state.step, Step::Main2);
            assert_eq!(count(&state, actor), 0);
            assert_eq!(effective_normal_cast_cost(def, actor, &state).generic, 4);
            for land in &lands[6..9] {
                step(&mut state, Action::ActivateManaAbility(*land)).unwrap();
            }
            step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { spell, legal_targets, .. }
                if spell == source && legal_targets.contains(&Target::Object(alternate)))
            );
            step(&mut state, Action::ChooseTarget(Target::Object(alternate))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert!(state.engine.pending_activation.is_none());
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert_projection(&state);
            let equipped = state.clone();
            settle(&mut state);
            assert_eq!(
                state.objects.get(source).v4.attached_to.unwrap().object,
                alternate
            );
            let result = state.clone();
            state = serde_json::from_slice(&serde_json::to_vec(&equipped).unwrap()).unwrap();
            settle(&mut state);
            assert_eq!(state, result);
            assert!(!has_effective_keyword(
                &state,
                host,
                Keywords::DOUBLE_STRIKE
            ));
            assert!(!has_effective_keyword(&state, host, Keywords::TRAMPLE));
        }
    }
}

#[test]
fn genuine_paid_flash_on_opponents_turn_and_targetless_entry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor.opponent());
        let source = put(&mut state, actor, "Embercleave", Zone::Hand);
        let lands = (0..6)
            .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
            .collect::<Vec<_>>();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. }
            if player == actor.opponent())
        );
        step(&mut state, Action::Pass).unwrap();
        // Six total mana with only one red cannot pay the printed RR. These
        // additional basic lands are setup in an independent negative world.
        let mut wrong_colors = state.clone();
        let forests = (0..5)
            .map(|_| put(&mut wrong_colors, actor, "Forest", Zone::Battlefield))
            .collect::<Vec<_>>();
        for land in forests {
            step(&mut wrong_colors, Action::ActivateManaAbility(land)).unwrap();
        }
        step(&mut wrong_colors, Action::ActivateManaAbility(lands[0])).unwrap();
        assert_eq!(
            wrong_colors.players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum::<u32>(),
            6
        );
        assert!(
            matches!(next(&mut wrong_colors), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && !castable_spells.contains(&source))
        );
        for land in lands {
            step(&mut state, Action::ActivateManaAbility(land)).unwrap();
        }
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
            if player == actor && castable_spells.contains(&source))
        );
        step(&mut state, Action::CastSpell(source)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert!(state.engine.pending_cast.is_none());
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(source).zone, Zone::Stack);
        assert_projection(&state);
        let paid = state.clone();
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert!(state.objects.get(source).v4.attached_to.is_none());
        assert!(state.engine.pending_triggers.is_empty());
        assert_eq!(state.active_player, actor.opponent());
        let complete = state.clone();
        state = serde_json::from_slice(&serde_json::to_vec(&paid).unwrap()).unwrap();
        settle(&mut state);
        assert_eq!(state, complete);
    }
}

fn waiting_from(
    actor: PlayerId,
    mut state: GameState,
) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let source = put(&mut state, actor, meaning::NAME, Zone::Hand);
    let host = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let alternate = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    crate::graveyard_return_public_v61::tests::announce(&mut state, actor, source, None);
    assert_projection(&state);
    assert_eq!(capture(&state, actor).unwrap().casts.len(), 1);
    for _ in 0..40 {
        match next(&mut state) {
            Decision::ChooseTargets { spell, .. } if spell == source => {
                assert_projection(&state);
                let facts = capture(&state, actor).unwrap();
                let row = facts
                    .abilities
                    .iter()
                    .find(|r| r.kind == AbilityKindV65::EntryAttach)
                    .unwrap();
                assert_eq!(row.stage, StageV65::ChoosingEntryTarget);
                assert!(!row.selection_complete);
                assert!(row
                    .current_candidate_pool
                    .iter()
                    .any(|r| r.object == host.0));
                return (state, source, host, alternate);
            }
            Decision::CastSpellOrPass { .. } => step(&mut state, Action::Pass).unwrap(),
            other => panic!("unexpected genuine Ember entry publication: {other:?}"),
        }
    }
    panic!("genuine Ember entry target choice absent");
}
fn waiting(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = crate::graveyard_return_public_v61::tests::ready(actor);
    put(
        &mut state,
        actor.opponent(),
        "High Fae Trickster",
        Zone::Battlefield,
    );
    waiting_from(actor, state)
}
fn publish(state: &mut GameState, host: ObjectId) -> crate::ids::StackItemId {
    step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let row = state.stack.last().unwrap();
    let id = row.v4.stack_item_id;
    assert_eq!(row.targets, [Target::Object(host)]);
    let facts = capture(state, row.controller).unwrap();
    let public = facts
        .abilities
        .iter()
        .find(|r| r.stage == StageV65::Published)
        .unwrap();
    assert!(public.selection_complete && public.payment_complete);
    assert_eq!(public.original_targets.len(), 1);
    assert!(public.original_targets[0].currently_legal);
    assert_projection(state);
    id
}
fn attached(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let (mut state, source, host, alternate) = waiting(actor);
    publish(&mut state, host);
    crate::graveyard_return_public_v61::tests::settle(&mut state);
    assert_eq!(
        state.objects.get(source).v4.attached_to.unwrap().object,
        host
    );
    assert_eq!(engine::checked_effective_pt(&state, host).unwrap(), (2, 2));
    assert_projection(&state);
    (state, source, host, alternate)
}
#[test]
fn closed_complete_equipment_program_keeps_partial_and_refuses_changed_meaning() {
    let value = meaning::printed_meaning().unwrap();
    assert_eq!(value.card_id, 462);
    assert_eq!(
        value.printed.colored_mana,
        [mana::ManaColor::R, mana::ManaColor::R]
    );
    assert_eq!(value.ordered_rules.len(), 5);
    assert_eq!(card_def::CARD_DEFS[462].capability, CardCapability::Partial);
    assert!(
        !value.full_engine_admitted
            && !value.owning_consumer_admitted
            && !value.model_admitted
            && !value.sampler_admitted
    );
    meaning::validate(&value).unwrap();
    let mut wrong = value.clone();
    wrong.ordered_rules.reverse();
    assert!(meaning::validate(&wrong).is_err());
    let mut wrong = value.clone();
    wrong.printed.colored_mana.pop();
    assert!(meaning::validate(&wrong).is_err());
    let mut wire = serde_json::to_value(&value).unwrap();
    wire["execution_permit"] = true.into();
    assert!(serde_json::from_value::<meaning::MeaningV65>(wire).is_err());
}
#[test]
fn genuine_paid_entry_responses_preserve_captured_controller_and_target_incarnation_both_seats() {
    use crate::graveyard_return_public_v61::tests::{announce, quiet as respond, settle};
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut state, source, host, _) = waiting(actor);
            let original = publish(&mut state, host);
            let original_row = state.stack.last().unwrap().clone();
            if mode == 2 {
                // Explicit native control primitive, not an illegal Employment
                // cast at an Artifact. The original source was genuinely paid.
                crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
            } else {
                let spell = put(
                    &mut state,
                    actor.opponent(),
                    if mode == 0 {
                        "Murder"
                    } else {
                        "Involuntary Employment"
                    },
                    Zone::Hand,
                );
                announce(&mut state, actor.opponent(), spell, Some(host));
            }
            assert_projection(&state);
            for _ in 0..60 {
                if state
                    .stack
                    .last()
                    .is_some_and(|row| row.v4.stack_item_id == original)
                {
                    break;
                }
                let decision = next(&mut state);
                respond(&mut state, decision);
            }
            assert_eq!(state.stack.last(), Some(&original_row));
            let facts = capture(&state, actor).unwrap();
            let row = facts
                .abilities
                .iter()
                .find(|row| row.stage == StageV65::Published)
                .unwrap();
            assert_eq!(row.source.captured_controller, actor.into());
            assert_eq!(row.source.original_source.controller, actor.into());
            if mode < 2 {
                assert!(!row.original_targets[0].currently_legal);
                if mode == 0 {
                    assert!(row.original_targets[0].current_original_target.is_none());
                }
                settle(&mut state);
                assert!(state.objects.get(source).v4.attached_to.is_none());
            } else {
                assert_eq!(state.objects.get(source).controller, actor.opponent());
                assert!(row.original_targets[0].currently_legal);
                settle(&mut state);
                assert_eq!(
                    state.objects.get(source).v4.attached_to.unwrap().object,
                    host
                );
            }
            assert_projection(&state);
        }
    }
}
#[test]
fn genuine_paid_native_source_suppression_facedown_copy_and_restored_grants_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, host, _) = attached(actor);
        let mut suppressed = assert_projection(&state);
        // Explicit native suppression operand on genuinely paid equipment.
        // This does not claim Embercleave or High Fae has a removal ability.
        let suppressor = suppressed.players[actor.opponent().index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| {
                card_def::CARD_DEFS[usize::from(suppressed.objects.get(*id).card_def)].name
                    == "High Fae Trickster"
            })
            .unwrap();
        let link = crate::state::ObjectLinkV4 {
            object: suppressor,
            zone_change_count: suppressed.objects.get(suppressor).zone_change_count,
        };
        let timestamp = engine::next_timestamp(&mut suppressed);
        suppressed
            .objects
            .get_mut(source)
            .v4
            .creature_upgrade
            .get_or_insert_with(Default::default)
            .suppressed_by
            .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                source: link,
                timestamp,
            });
        let before = bytes(&suppressed);
        assert!(engine::attached_equipment_profiles(&suppressed, host)
            .next()
            .is_none());
        assert_eq!(
            engine::checked_effective_pt(&suppressed, host).unwrap(),
            (1, 1)
        );
        for keyword in [Keywords::DOUBLE_STRIKE, Keywords::TRAMPLE] {
            assert!(!engine::has_effective_keyword(&suppressed, host, keyword));
        }
        assert_projection(&suppressed);
        assert_eq!(bytes(&suppressed), before);
        event::propose_and_commit(
            &mut suppressed,
            ProposedEvent::zone_change(suppressor, Zone::Hand),
        );
        assert_eq!(
            engine::checked_effective_pt(&suppressed, host).unwrap(),
            (2, 2)
        );
        assert!(engine::has_effective_keyword(
            &suppressed,
            host,
            Keywords::DOUBLE_STRIKE
        ));
        assert_projection(&suppressed);
        // Explicit neutral face-down state operand on the same paid history.
        let mut facedown = assert_projection(&state);
        facedown.objects.get_mut(source).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers: 0,
            hidden_by: None,
            hidden_by_source: None,
        });
        assert!(engine::attached_equipment_profiles(&facedown, host)
            .next()
            .is_none());
        assert_eq!(
            engine::checked_effective_pt(&facedown, host).unwrap(),
            (1, 1)
        );
        let facts = capture(&facedown, actor).unwrap();
        assert!(!facts.authorized.iter().any(|r| r.object == source.0));
        assert_projection(&facedown);
        // A genuine native copy operation retains the copied definition and
        // its own independent incarnation; it begins unattached.
        let mut copied = assert_projection(&state);
        let copy =
            crate::standard_cards_v1::create_token_copy(&mut copied, source, actor.opponent())
                .unwrap();
        let facts = capture(&copied, actor).unwrap();
        assert!(facts.authorized.iter().any(|r| r.object == copy.0));
        assert!(copied.objects.get(copy).v4.attached_to.is_none());
        assert_projection(&copied);
    }
}
#[test]
fn genuine_paid_public_binding_forgeries_refuse_readonly_and_counters_keep_full_width_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, host, _) = waiting(actor);
        publish(&mut state, host);
        let original = bytes(&state);
        let facts = capture(&state, actor).unwrap();
        let mut wrong = facts.clone();
        wrong.current_cast_reduction[actor.index()].count = u32::MAX;
        assert!(validate(&state, actor, &wrong).is_err());
        let mut wrong = facts.clone();
        wrong.abilities[0].source.captured_controller = actor.opponent().into();
        assert!(validate(&state, actor, &wrong).is_err());
        let mut wire = serde_json::to_value(&facts).unwrap();
        wire["abilities"][0]["native_stack_id"] = 99.into();
        assert!(serde_json::from_value::<FactsV65>(wire).is_err());
        for mode in 0..4 {
            let mut forged = assert_projection(&state);
            let row = forged.stack.last_mut().unwrap();
            match mode {
                0 => row.controller = actor.opponent(),
                1 => {
                    row.v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                2 => row.v4.target_contracts.clear(),
                3 => row.inline_effect = Some(EffectOp::Sequence(vec![])),
                _ => unreachable!(),
            }
            let before = bytes(&forged);
            assert!(capture(&forged, actor).is_err());
            assert_eq!(bytes(&forged), before);
        }
        assert_eq!(bytes(&state), original);
        crate::graveyard_return_public_v61::tests::settle(&mut state);
        // Explicit full-width counter-storage primitives on a noncreature.
        let land = state.players[actor.index()]
            .battlefield
            .iter()
            .copied()
            .find(|id| state.objects.get(*id).card_def == card_id_by_name("Forest").unwrap())
            .unwrap();
        let o = state.objects.get_mut(land);
        o.counters = crate::state::Counters {
            plus1_plus1: i32::MAX,
            minus1_minus1: i16::MAX,
            minus0_minus1: i16::MAX,
            stun: i16::MAX,
            lore: i16::MAX,
            oil: i16::MAX,
            charge: i32::MAX,
            net: i32::MAX,
            bait: i32::MAX,
            soul: i32::MAX,
            incubation: i32::MAX,
            revival: i32::MAX,
            fellowship: i32::MAX,
        };
        o.v4.lifelink_keyword_counters = i16::MAX;
        o.v4.time_counters_v1 = u8::MAX;
        o.v4.creature_upgrade
            .get_or_insert_with(Default::default)
            .finality = u16::MAX;
        let ajani = put(
            &mut state,
            actor,
            "Ajani, Caller of the Pride",
            Zone::Battlefield,
        );
        let existing = crate::planeswalker_v1::loyalty(&state, ajani).unwrap();
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(
            &mut state,
            ajani,
            u32::MAX - existing,
        )
        .unwrap();
        let facts = capture(&state, actor).unwrap();
        let c = &facts.battlefield[actor.index()]
            .iter()
            .find(|r| r.card.object == land.0)
            .unwrap()
            .counters;
        assert_eq!(
            (
                c.plus_one,
                c.charge,
                c.net,
                c.bait,
                c.soul,
                c.incubation,
                c.revival,
                c.fellowship
            ),
            (
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX,
                i32::MAX
            )
        );
        assert_eq!(
            (
                c.minus_one,
                c.minus_toughness,
                c.stun,
                c.lore,
                c.oil,
                c.lifelink
            ),
            (i16::MAX, i16::MAX, i16::MAX, i16::MAX, i16::MAX, i16::MAX)
        );
        assert_eq!((c.time, c.finality), (u8::MAX, u16::MAX));
        assert_eq!(
            facts.battlefield[actor.index()]
                .iter()
                .find(|r| r.card.object == ajani.0)
                .unwrap()
                .counters
                .loyalty,
            u32::MAX
        );
        assert_projection(&state);
        let mut negative = assert_projection(&state);
        negative.objects.get_mut(land).counters.fellowship = -1;
        let before = bytes(&negative);
        assert!(capture(&negative, actor).is_err());
        assert_eq!(bytes(&negative), before);
    }
}
#[test]
fn independent_hidden_worlds_before_genuine_paid_cast_keep_public_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hidden: &str| {
            let mut state = crate::graveyard_return_public_v61::tests::ready(actor);
            put(&mut state, actor.opponent(), hidden, Zone::Hand);
            put(&mut state, actor.opponent(), hidden, Zone::Library);
            let face = put(&mut state, actor.opponent(), hidden, Zone::Battlefield);
            state.objects.get_mut(face).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            waiting_from(actor, state)
        };
        let (a, source, host, _) = build(meaning::NAME);
        let (b, other_source, other_host, _) = build("Island");
        assert_eq!((source, host), (other_source, other_host));
        assert_eq!(capture(&a, actor).unwrap(), capture(&b, actor).unwrap());
        assert_eq!(
            capture(&assert_projection(&a), actor).unwrap(),
            capture(&assert_projection(&b), actor).unwrap()
        );
    }
}

#[test]
fn genuine_paid_witness_host_removal_and_later_equip_grants_follow_timestamps_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, host, alternate) = attached(actor);
        let aura = crate::graveyard_return_public_v61::tests::paid(
            &mut state,
            actor,
            "Witness Protection",
            Some(host),
        );
        assert_eq!(state.objects.get(aura).v4.attached_to.unwrap().object, host);
        assert_eq!(engine::checked_effective_pt(&state, host).unwrap(), (2, 2));
        assert!(!engine::has_effective_keyword(
            &state,
            host,
            Keywords::DOUBLE_STRIKE
        ));
        assert!(!engine::has_effective_keyword(
            &state,
            host,
            Keywords::TRAMPLE
        ));
        assert_projection(&state);
        for target in [alternate, host] {
            let before = state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|&&id| state.objects.get(id).tapped)
                .count();
            step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
            assert!(matches!(next(&mut state),Decision::ChooseTargets{spell,..}if spell==source));
            let facts = capture(&state, actor).unwrap();
            let row = facts
                .abilities
                .iter()
                .find(|r| r.kind == AbilityKindV65::Equip)
                .unwrap();
            assert_eq!(row.stage, StageV65::PayingEquip);
            assert!(!row.payment_complete && !row.selection_complete);
            assert_projection(&state);
            step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass{player,..}if player==actor)
            );
            assert!(state.engine.pending_activation.is_none());
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert_eq!(
                state.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| state.objects.get(id).tapped)
                    .count()
                    - before,
                3
            );
            assert_projection(&state);
            crate::graveyard_return_public_v61::tests::settle(&mut state);
            assert_eq!(
                state.objects.get(source).v4.attached_to.unwrap().object,
                target
            );
            assert_projection(&state);
        }
        assert!(engine::has_effective_keyword(
            &state,
            host,
            Keywords::DOUBLE_STRIKE
        ));
        assert!(engine::has_effective_keyword(
            &state,
            host,
            Keywords::TRAMPLE
        ));
        assert_eq!(engine::checked_effective_pt(&state, host).unwrap(), (2, 2));
    }
}
