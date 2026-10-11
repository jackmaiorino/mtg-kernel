//! Future printed acceptance requires actual registration, without injected roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::{
    self, EffectCond, EffectOp, EffectTargetSelectionPurpose, PendingEffectChoice,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("printed acceptance requires registration: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn ready(actor: PlayerId, size: usize) -> GameState {
    let cards = ["Forest", "Plains", "Island", "Mountain", "Swamp"]
        .map(|name| card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards[..size],
        &cards[..size],
        |id| CARD_DEFS[id as usize].name.into(),
        11006,
        actor,
    );
    state.step = Step::Main1;
    for seat in [actor, actor.opponent()] {
        put(&mut state, seat, "Mountain", Zone::Battlefield);
        put(&mut state, seat, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..40 {
        let decision = next(state);
        match decision {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => return Some(other),
        }
    }
    panic!("resolution did not settle");
}
fn raid(state: &mut GameState, actor: PlayerId, nonempty: bool) {
    let attacker = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    assert!(matches!(next(state), Decision::DeclareAttackers { player, .. } if player == actor));
    engine::step(
        state,
        Action::DeclareAttackers(if nonempty { vec![attacker] } else { vec![] }),
    )
    .unwrap();
    for _ in 0..40 {
        let decision = next(state);
        if state.step == Step::Main2
            && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    panic!("Main2 did not retain a castable decision");
}
fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Gutless Plunderer", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == source));
    source
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn choose(state: &mut GameState, cards: &[ObjectId]) {
    for &card in cards {
        if state
            .engine
            .pending_effect
            .as_ref()
            .unwrap()
            .choice
            .is_none()
        {
            break;
        }
        engine::step(state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
    }
    if state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_some()
    {
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
}
#[test]
fn exact_printed_definition_owns_terminal_raid_and_not_ordinary_surveil() {
    let id = card_id_by_name("Gutless Plunderer").expect("true registration required");
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Skeleton, Subtype::Pirate]);
    assert_eq!(
        (def.cost.generic, def.mana_value, def.power, def.toughness),
        (2, 3, Some(2), Some(2))
    );
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert!(def.keywords.has(Keywords::DEATHTOUCH));
    let selector = mtg_kernel::rules_meaning_raid_entry_v1::selector_v1().unwrap();
    let meaning = mtg_kernel::rules_meaning_raid_entry_v1::meaning_v1(&selector, id, 0).unwrap();
    assert_eq!(
        meaning.instruction,
        mtg_kernel::rules_meaning_raid_entry_v1::InstructionV1::PrivateLook {
            count: 3,
            retained_min: 0,
            retained_max: 1,
            remainder_graveyard_ordered: true
        }
    );
    let mut bad = meaning;
    bad.instruction = mtg_kernel::rules_meaning_raid_entry_v1::InstructionV1::PrivateLook {
        count: 3,
        retained_min: 0,
        retained_max: 3,
        remainder_graveyard_ordered: true,
    };
    assert!(mtg_kernel::rules_meaning_raid_entry_v1::validate_v1(&bad).is_err());
    let triggers = mtg_kernel::trigger::triggers_for(id);
    assert_eq!(triggers.len(), 1);
    assert_eq!(
        triggers[0].condition,
        mtg_kernel::trigger::TriggerCondition::EtbAfterControllerAttackedThisTurn
    );
    assert_eq!(
        (triggers[0].effect)(),
        EffectOp::Conditional {
            cond: EffectCond::ControllerAttackedThisTurn,
            then: Box::new(EffectOp::LookTopMayKeepOneRestGraveyard { count: 3 }),
            else_: Box::new(EffectOp::Sequence(vec![])),
        }
    );
}
#[test]
fn actual_both_seat_casts_keep_zero_or_one_short_empty_and_exact_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for size in 0..=5 {
            for kept in std::iter::once(None).chain((0..size.min(3)).map(Some)) {
                let mut state = ready(actor, size);
                raid(&mut state, actor, true);
                let original = state.players[actor.index()].library.clone();
                let source = cast(&mut state, actor);
                let mut announced = replay(&state);
                let decision = settle(&mut state);
                assert_eq!(decision, settle(&mut announced));
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&announced).unwrap()
                );
                assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                let n = size.min(3);
                if n == 0 {
                    assert!(decision.is_none());
                    continue;
                }
                assert!(
                    matches!(decision, Some(Decision::ChooseEffectTargets { player, min_targets, max_targets, .. })
                    if player == actor && usize::from(min_targets) == n.saturating_sub(1) && usize::from(max_targets) == n)
                );
                let graveyard = original[..n]
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| Some(*index) != kept)
                    .map(|(_, id)| *id)
                    .rev()
                    .collect::<Vec<_>>();
                let mut restored = replay(&state);
                choose(&mut state, &graveyard);
                choose(&mut restored, &graveyard);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
                let mut answered = replay(&state);
                assert!(settle(&mut state).is_none());
                assert!(settle(&mut answered).is_none());
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&answered).unwrap()
                );
                assert_eq!(state.players[actor.index()].graveyard, graveyard);
                let expected = kept
                    .map(|index| original[index])
                    .into_iter()
                    .chain(original[n..].iter().copied())
                    .collect::<Vec<_>>();
                assert_eq!(state.players[actor.index()].library, expected);
            }
        }
    }
}
#[test]
fn multi_top_unlooked_duplicates_and_restore_authority_tampering_refuse_without_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let original = state.players[actor.index()].library.clone();
        cast(&mut state, actor);
        settle(&mut state).unwrap();
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(original[0])),
        )
        .unwrap();
        let before = serde_json::to_vec(&state).unwrap();
        for action in [
            Action::FinishEffectSelection,
            Action::ChooseEffectTarget(Target::Object(original[0])),
            Action::ChooseEffectTarget(Target::Object(original[3])),
        ] {
            assert!(engine::step(&mut state, action).is_err());
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        for mutation in 0..6 {
            let mut corrupt = replay(&state);
            match mutation {
                0 => {
                    let pending = corrupt.engine.pending_effect.as_mut().unwrap();
                    pending.choice = None;
                    pending.frames.clear();
                    pending.answered_choice_guard = None;
                }
                1 => {
                    if let Some(PendingEffectChoice::SelectTargets { min_targets, .. }) = corrupt
                        .engine
                        .pending_effect
                        .as_mut()
                        .unwrap()
                        .choice
                        .as_mut()
                    {
                        *min_targets = 0;
                    }
                }
                2 => {
                    corrupt
                        .engine
                        .pending_effect
                        .as_mut()
                        .unwrap()
                        .ctx
                        .controller = actor.opponent();
                }
                3 => {
                    corrupt.players[actor.index()].library.swap(3, 4);
                    let pending = corrupt.engine.pending_effect.as_mut().unwrap();
                    if let Some(PendingEffectChoice::SelectTargets {
                        purpose:
                            EffectTargetSelectionPurpose::SurveilLibraryMany {
                                original_library, ..
                            },
                        ..
                    }) = pending.choice.as_mut()
                    {
                        original_library.swap(3, 4);
                    }
                }
                4 => {
                    corrupt.engine.event_history.retain(|event| {
                        !matches!(
                            event,
                            event::CommittedEvent::PrivateKeepOneLibraryCapturedV1 { .. }
                        )
                    });
                }
                5 => {
                    corrupt.objects.get_mut(original[1]).zone_change_count += 1;
                }
                _ => unreachable!(),
            }
            let before = serde_json::to_vec(&corrupt).unwrap();
            assert!(effect::validate_pending_effect_choice(&corrupt).is_err());
            assert_eq!(before, serde_json::to_vec(&corrupt).unwrap());
            assert!(matches!(
                engine::advance_until_decision(&mut corrupt),
                Decision::Halted { .. }
            ));
        }
    }
}
#[test]
fn raid_no_attack_empty_declaration_other_seat_and_stale_resolution_do_not_look() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for history in 0..4 {
            let mut state = ready(actor, 5);
            if history == 1 {
                raid(&mut state, actor, false);
            }
            if history == 2 {
                state.active_player = actor.opponent();
                state.priority_player = actor.opponent();
                raid(&mut state, actor.opponent(), true);
                state.active_player = actor;
                state.priority_player = actor;
                state.step = Step::Main2;
            }
            if history == 3 {
                raid(&mut state, actor, true);
            }
            let original = state.players[actor.index()].library.clone();
            cast(&mut state, actor);
            if history == 3 {
                state.attack_declaration_turn_v1 = None;
            }
            assert!(settle(&mut state).is_none());
            assert_eq!(state.players[actor.index()].library, original);
            assert!(state.known_library_cards(actor, actor).is_empty());
        }
    }
}
#[test]
fn captured_controller_and_departed_source_keep_the_original_library_and_chooser() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for departed in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let source = put(&mut state, actor, "Gutless Plunderer", Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            let pending = mtg_kernel::trigger::collect_and_process(&mut state);
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].controller, actor);
            state.engine.pending_triggers.extend(pending);
            if departed {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
            } else {
                state.players[actor.index()]
                    .battlefield
                    .retain(|id| *id != source);
                state.players[actor.opponent().index()]
                    .battlefield
                    .push(source);
                state.objects.get_mut(source).controller = actor.opponent();
            }
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor)
            );
            let original = state.players[actor.index()].library.clone();
            choose(&mut state, &original[..3]);
            assert!(settle(&mut state).is_none());
            assert_eq!(state.players[actor.index()].library, original[3..]);
            assert!(state.players[actor.opponent().index()].graveyard.is_empty());
        }
    }
}
#[test]
fn pending_and_answered_private_menus_are_invariant_for_the_nonchooser() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        cast(&mut state, actor);
        settle(&mut state).unwrap();
        let surface = PolicySurfaceV5::new();
        let public = mtg_kernel::policy_observation_v9::observe_policy_v9(
            &state,
            &surface,
            actor.opponent(),
            0,
            0,
            0,
            1,
        )
        .unwrap();
        let original = state.players[actor.index()].library.clone();
        for keep_one in [false, true] {
            let mut branch = replay(&state);
            choose(&mut branch, &original[..if keep_one { 2 } else { 3 }]);
            assert_eq!(
                public,
                mtg_kernel::policy_observation_v9::observe_policy_v9(
                    &branch,
                    &surface,
                    actor.opponent(),
                    0,
                    0,
                    0,
                    1
                )
                .unwrap()
            );
            // Different genuine hidden library before capture, rather than a
            // forged post-capture definition that authentication must reject.
            let mut hidden = ready(actor, 5);
            let card = hidden.players[actor.index()].library[0];
            let alternative = card_id_by_name("Counterspell").unwrap();
            hidden.objects.get_mut(card).card_def = alternative;
            hidden.objects.get_mut(card).name = "Counterspell".into();
            hidden.objects.get_mut(card).v4 = ObjectStateV4::from_card_def(alternative);
            raid(&mut hidden, actor, true);
            cast(&mut hidden, actor);
            settle(&mut hidden).unwrap();
            let cards = hidden.players[actor.index()].library.clone();
            choose(&mut hidden, &cards[..if keep_one { 2 } else { 3 }]);
            assert_eq!(
                public,
                mtg_kernel::policy_observation_v9::observe_policy_v9(
                    &hidden,
                    &surface,
                    actor.opponent(),
                    0,
                    0,
                    0,
                    1
                )
                .unwrap()
            );
        }
    }
}

#[test]
fn physical_owner_differs_from_entry_controller_and_raid_belongs_to_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let source = put(
            &mut state,
            actor.opponent(),
            "Gutless Plunderer",
            Zone::Hand,
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(source, actor),
        );
        assert_eq!(state.objects.get(source).owner, actor.opponent());
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor)
        );
        let cards = state.players[actor.index()].library[..3].to_vec();
        choose(&mut state, &cards);
        assert!(settle(&mut state).is_none());
        assert_eq!(state.players[actor.index()].graveyard, cards);
        assert!(state.players[actor.opponent().index()].graveyard.is_empty());
    }
}
#[test]
fn actual_ability_removal_suppresses_deathtouch_and_keeps_already_captured_raid_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
        raid(&mut state, actor, true);
        let source = cast(&mut state, actor);
        let mut entered = false;
        for _ in 0..20 {
            let decision = next(&mut state);
            if state.objects.get(source).zone == Zone::Battlefield {
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
                );
                entered = true;
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(entered);
        assert!(engine::has_effective_keyword(
            &state,
            source,
            Keywords::DEATHTOUCH
        ));
        let aura = put(&mut state, actor, "Witness Protection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&aura))
        );
        engine::step(&mut state, Action::CastSpell(aura)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let mut attached = false;
        for _ in 0..20 {
            let decision = next(&mut state);
            if state.objects.get(aura).zone == Zone::Battlefield {
                attached = true;
                break;
            }
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(attached);
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::DEATHTOUCH
        ));
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor)
        );
        let cards = state.players[actor.index()].library[..3].to_vec();
        choose(&mut state, &cards);
        assert!(settle(&mut state).is_none());
        assert_eq!(state.players[actor.index()].graveyard, cards);
    }
}

#[test]
fn manufactured_closed_boundary_cannot_skip_the_actual_private_graveyard_operation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        cast(&mut state, actor);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        let original_library = state.players[actor.index()].library.clone();
        let last = state
            .engine
            .event_history
            .iter()
            .rev()
            .find_map(|event| match event {
                event::CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt } => {
                    Some(receipt.clone())
                }
                _ => None,
            })
            .unwrap();
        let mut corrupt = replay(&state);
        corrupt
            .stack
            .retain(|item| item.v4.stack_item_id != last.stack_item_id);
        corrupt.engine.pending_effect = None;
        let mut forged = last.clone();
        forged.history_index = u32::try_from(corrupt.engine.event_history.len()).unwrap();
        forged.previous_history_index = Some(last.history_index);
        forged.ordinal += 1;
        forged.pending = None;
        corrupt
            .engine
            .event_history
            .push(event::CommittedEvent::SquadRallierResolutionBoundaryV1 { receipt: forged });
        for _ in 0..2 {
            let mut restored = replay(&corrupt);
            let before = serde_json::to_vec(&restored).unwrap();
            assert!(effect::validate_pending_effect_choice(&restored).is_err());
            assert!(engine::step(&mut restored, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&restored).unwrap());
            assert_eq!(restored.players[actor.index()].library, original_library);
        }
        let selected = original_library[..2].to_vec();
        choose(&mut state, &selected);
        assert!(settle(&mut state).is_none());
        effect::validate_pending_effect_choice(&replay(&state)).unwrap();
        for mutation in 0..3 {
            let mut corrupt = replay(&state);
            match mutation {
                0 => corrupt.engine.event_history.retain(|event| {
                    !matches!(
                        event,
                        event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 { .. }
                    )
                }),
                1 => {
                    let event = corrupt
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|event| {
                            matches!(
                                event,
                                event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 { .. }
                            )
                        })
                        .unwrap();
                    if let event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 {
                        move_history_indices,
                        ..
                    } = event
                    {
                        move_history_indices.clear();
                    }
                }
                2 => {
                    let event = corrupt
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|event| {
                            matches!(
                                event,
                                event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 { .. }
                            )
                        })
                        .unwrap();
                    if let event::CommittedEvent::PrivateKeepOneLibraryConsumedV1 {
                        graveyard,
                        ..
                    } = event
                    {
                        graveyard[0] = graveyard[1];
                    }
                }
                _ => unreachable!(),
            }
            let mut restored = replay(&corrupt);
            let before = serde_json::to_vec(&restored).unwrap();
            assert!(engine::step(&mut restored, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&restored).unwrap());
        }
    }
}

#[test]
fn genuine_legend_waiting_raid_trigger_keeps_its_original_controller_before_legend_moves() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        put(
            &mut state,
            actor,
            "Dwynen, Gilt-Leaf Daen",
            Zone::Battlefield,
        );
        let second_legend = put(&mut state, actor, "Dwynen, Gilt-Leaf Daen", Zone::Hand);
        let source = put(&mut state, actor, "Gutless Plunderer", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(source, Zone::Battlefield),
                ProposedEvent::zone_change(second_legend, Zone::Battlefield),
            ],
        );
        let Decision::ChooseLegendPermanent { candidates, .. } = next(&mut state) else {
            panic!("actual duplicate legends must hold genuine entry triggers");
        };
        // Physical control-transfer boundary after the actual entry and capture.
        state.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[actor.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = actor.opponent();
        let mut json = serde_json::to_value(&state).unwrap();
        let waiting = json["pending_legend_rule_v1"]["waiting_triggers"]
            .as_array_mut()
            .unwrap();
        let row = waiting
            .iter_mut()
            .find(|row| row["source"] == serde_json::to_value(source).unwrap())
            .unwrap();
        assert_eq!(row["controller"], serde_json::to_value(actor).unwrap());
        row["controller"] = serde_json::to_value(actor.opponent()).unwrap();
        row["source_contract"]["controller"] = serde_json::to_value(actor.opponent()).unwrap();
        let mut corrupt: GameState = serde_json::from_value(json).unwrap();
        let before = serde_json::to_vec(&corrupt).unwrap();
        assert!(engine::step(&mut corrupt, Action::ChooseLegendPermanent(candidates[0])).is_err());
        assert_eq!(before, serde_json::to_vec(&corrupt).unwrap());
        let mut restored = replay(&corrupt);
        assert!(engine::step(&mut restored, Action::ChooseLegendPermanent(candidates[0])).is_err());
        assert_eq!(before, serde_json::to_vec(&restored).unwrap());
        engine::step(&mut state, Action::ChooseLegendPermanent(candidates[0])).unwrap();
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor)
        );
    }
}
