//! Native fixtures using the registered physical Shapeshifter.

use super::*;
use crate::engine::{self, Action, Decision, RuntimeRulesV1, UnsupportedMechanic};
use crate::mana::ManaColor;
use crate::state::{Step, Target};

fn ordered_game(player: PlayerId) -> GameState {
    let mut state = game();
    state.step = Step::Main1;
    state.active_player = player;
    state.priority_player = player;
    state.engine.runtime_rules = RuntimeRulesV1::OWNER_ORDERED_GRAVEYARD_V1;
    state
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

fn pool(state: &mut GameState, player: PlayerId, colors: &[ManaColor], generic: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    pool[ManaColor::C.pool_index()] += generic;
    for &color in colors {
        pool[color.pool_index()] += 1;
    }
}

fn cast(state: &mut GameState, spell: ObjectId) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    act(state, Action::CastSpell(spell));
}

fn cast_volrath(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, VOLRATH, Zone::Hand);
    pool(state, player, &[ManaColor::U, ManaColor::U], 1);
    cast(state, source);
    source
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..32 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("registered fixture did not settle");
}

fn settled(state: &mut GameState) {
    assert_eq!(settle(state), None);
}

fn resolve_untap(state: &mut GameState, player: PlayerId, source: ObjectId, land: ObjectId) {
    assert!(matches!(settle(state),
        Some(Decision::ChooseEffectTargets { player: chooser, source: host, legal_targets, .. })
            if chooser == player && host == source && legal_targets.contains(&Target::Object(land))));
    act(state, Action::ChooseEffectTarget(Target::Object(land)));
    if matches!(
        next(state),
        Decision::ChooseEffectTargets {
            can_finish: true,
            ..
        }
    ) {
        act(state, Action::FinishEffectSelection);
    }
    settled(state);
    assert!(!state.objects.get(land).tapped);
}

fn captured_entry(state: &GameState, source: ObjectId) -> Option<DonatedTriggerCaptureV1> {
    state
        .engine
        .event_history
        .iter()
        .find_map(|event| match event {
            CommittedEvent::FullTextTriggerCapturedV1 { capture }
                if capture.host.source == source =>
            {
                Some(**capture)
            }
            _ => None,
        })
}

#[test]
fn volrath_native_paid_donated_etb_obeys_torpor_orb_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for donor_name in ["Palinchron", "Karmic Guide"] {
            for suppressed in [false, true] {
                let mut state = ordered_game(player);
                let target = put(&mut state, player, "Tireless Tribe", Zone::Graveyard);
                let donor = put(&mut state, player, donor_name, Zone::Graveyard);
                let land = put(&mut state, player, "Island", Zone::Battlefield);
                state.objects.get_mut(land).tapped = true;
                if suppressed {
                    let orb = put(&mut state, player, "Torpor Orb", Zone::Hand);
                    pool(&mut state, player, &[], 2);
                    cast(&mut state, orb);
                    settled(&mut state);
                    assert_eq!(state.objects.get(orb).zone, Zone::Battlefield);
                    assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
                }

                let source = cast_volrath(&mut state, player);
                if suppressed {
                    settled(&mut state);
                    assert!(captured_entry(&state, source).is_none());
                    assert!(state.objects.get(land).tapped);
                    assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
                } else if donor_name == "Palinchron" {
                    resolve_untap(&mut state, player, source, land);
                } else {
                    assert!(matches!(settle(&mut state),
                        Some(Decision::ChooseTargets { player: chooser, spell, legal_targets, .. })
                            if chooser == player && spell == source && legal_targets.contains(&Target::Object(target))));
                    act(&mut state, Action::ChooseTarget(Target::Object(target)));
                    settled(&mut state);
                    assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
                }
                assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
                assert_eq!(active_text(&state, source).unwrap().donor().object, donor);
                if !suppressed {
                    let capture = captured_entry(&state, source).unwrap();
                    assert_eq!(
                        capture.host,
                        AbilitySourceContractV4::capture(&state, source)
                    );
                    assert_eq!(capture.donor.source, donor);
                }
            }
        }
    }
}

#[test]
fn volrath_native_simultaneous_entry_obeys_final_suppressor_scene_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for arriving in [true, false] {
            for orb_first in [true, false] {
                let mut state = ordered_game(player);
                let source = put(&mut state, player, VOLRATH, Zone::Hand);
                let donor = put(&mut state, player, "Palinchron", Zone::Graveyard);
                let land = put(&mut state, player, "Island", Zone::Battlefield);
                state.objects.get_mut(land).tapped = true;
                let (from, to) = if arriving {
                    (Zone::Hand, Zone::Battlefield)
                } else {
                    (Zone::Battlefield, Zone::Exile)
                };
                let orb = put(&mut state, player.opponent(), "Torpor Orb", from);
                let mut changes = vec![
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                    ProposedEvent::zone_change(orb, to),
                ];
                if orb_first {
                    changes.reverse();
                }
                event::propose_and_commit_batch(&mut state, changes);
                assert_eq!(state.objects.get(orb).zone, to);
                assert_eq!(active_text(&state, source).unwrap().donor().object, donor);
                if arriving {
                    settled(&mut state);
                    assert!(captured_entry(&state, source).is_none());
                    assert!(state.objects.get(land).tapped);
                } else {
                    resolve_untap(&mut state, player, source, land);
                    assert!(matches!(
                        captured_entry(&state, source).unwrap().kind,
                        DonatedTriggerKindV1::PalinchronEnteredAfterBatch { .. }
                    ));
                }
                // Archived native entry facts also require the successor
                // independently of the current runtime-mode bit.
                state.engine.runtime_rules = RuntimeRulesV1::default();
                let refusal =
                    "full-text trigger producers require a complete public consumer successor";
                assert_eq!(refuse_legacy(&state).unwrap_err(), refusal);
                assert_eq!(
                    crate::event_history_v1::guard_tests_v1::with_scans_forced(|| refuse_legacy(
                        &state
                    ),)
                    .unwrap_err(),
                    refusal
                );
            }
        }
    }
}

#[test]
fn volrath_native_simultaneous_entry_uses_final_donor_after_departure_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for donor_first in [true, false] {
            let mut state = ordered_game(player);
            let source = put(&mut state, player, VOLRATH, Zone::Hand);
            let target = put(&mut state, player, "Tireless Tribe", Zone::Graveyard);
            let guide = put(&mut state, player, "Karmic Guide", Zone::Graveyard);
            let palinchron = put(&mut state, player, "Palinchron", Zone::Graveyard);
            let mut changes = vec![
                ProposedEvent::zone_change(source, Zone::Battlefield),
                ProposedEvent::zone_change(palinchron, Zone::Exile),
            ];
            if donor_first {
                changes.reverse();
            }
            event::propose_and_commit_batch(&mut state, changes);
            assert_eq!(state.objects.get(palinchron).zone, Zone::Exile);
            assert_eq!(active_text(&state, source).unwrap().donor().object, guide);
            let capture = captured_entry(&state, source).unwrap();
            assert_eq!(
                capture.host,
                AbilitySourceContractV4::capture(&state, source)
            );
            assert_eq!(
                capture.donor,
                AbilitySourceContractV4::capture(&state, guide)
            );
            assert!(matches!(
                capture.kind,
                DonatedTriggerKindV1::KarmicGuideEnteredAfterBatch { .. }
            ));

            let prompt = settle(&mut state).expect("the final donor offers a graveyard target");
            assert!(matches!(&prompt,
                Decision::ChooseTargets { player: chooser, spell, legal_targets, .. }
                    if *chooser == player && *spell == source && legal_targets.contains(&Target::Object(target))));
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(next(&mut restored), prompt);
            for branch in [&mut state, &mut restored] {
                act(branch, Action::ChooseTarget(Target::Object(target)));
                settled(branch);
                assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
                assert_eq!(branch.players[player.index()].graveyard, [guide]);
            }
            assert_eq!(state.state_hash(), restored.state_hash());
        }
    }
}

#[test]
fn volrath_native_registration_retains_paid_discard_with_no_or_unsupported_text_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        // Glory supplies characteristics, and its only activation works from
        // the graveyard, so no donated slot is offered. An unsupported donor
        // such as Masticore is refused at entry (see the refusal fixtures).
        for donor_name in [None, Some(VOLRATH), Some("Glory")] {
            let mut state = ordered_game(player);
            let donor = donor_name.map(|name| put(&mut state, player, name, Zone::Graveyard));
            let discarded = put(&mut state, player, "Island", Zone::Hand);
            let kept = put(&mut state, player, "Forest", Zone::Hand);
            let source = cast_volrath(&mut state, player);
            settled(&mut state);
            assert_eq!(state.objects.get(source).card_def, id(VOLRATH));
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            assert_eq!(
                active_text(&state, source).map(|text| text.donor().object),
                donor
            );
            if donor_name != Some("Glory") {
                assert_eq!(engine::effective_power(&state, source), 0);
                assert_eq!(engine::effective_toughness(&state, source), 1);
            }
            if donor_name.is_none() {
                let mut legacy = state.clone();
                legacy.engine.runtime_rules = RuntimeRulesV1::default();
                for observer in [player, player.opponent()] {
                    assert!(crate::rl::observe_v1(&legacy, observer, 0)
                        .unwrap_err()
                        .0
                        .contains("full-text characteristics"));
                }
            }

            pool(&mut state, player, &[], 2);
            let Decision::CastSpellOrPass {
                activatable_abilities,
                ..
            } = next(&mut state)
            else {
                panic!("expected native activation choices");
            };
            assert_eq!(
                activatable_abilities
                    .iter()
                    .copied()
                    .filter(|(object, _)| *object == source)
                    .collect::<Vec<_>>(),
                [(source, 0)],
            );
            act(&mut state, Action::ActivateAbility(source, 0));
            assert!(
                matches!(settle(&mut state), Some(Decision::Discard { player: chooser, choices, count: 1 })
                if chooser == player && choices.contains(&discarded) && choices.contains(&kept))
            );
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            act(&mut state, Action::Discard(vec![discarded]));
            settled(&mut state);
            assert_eq!(
                state.players[player.index()].graveyard.last(),
                Some(&discarded)
            );
            assert!(state.players[player.index()].hand.contains(&kept));
            assert!(active_text(&state, source).is_none());
        }
    }
}

#[test]
fn volrath_native_mixed_batch_records_real_entry_and_order_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        let source = put(&mut state, player, VOLRATH, Zone::Hand);
        let donor = put(&mut state, player, "Palinchron", Zone::Library);
        let other = put(&mut state, player, "Triskelion", Zone::Library);
        let land = put(&mut state, player, "Island", Zone::Battlefield);
        state.objects.get_mut(land).tapped = true;
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(source, Zone::Battlefield),
                ProposedEvent::zone_change(donor, Zone::Graveyard),
                ProposedEvent::zone_change(other, Zone::Graveyard),
            ],
        );
        let prompt = next(&mut state);
        assert!(
            matches!(&prompt, Decision::ChooseEffectTargets { player: chooser, legal_targets, .. }
            if *chooser == player && legal_targets.as_slice() == [Target::Object(donor), Target::Object(other)])
        );
        assert_eq!(state.objects.get(source).zone, Zone::Hand);
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event, CommittedEvent::FullTextBatchEntryV1 { .. })));
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(next(&mut restored), prompt);
        for branch in [&mut state, &mut restored] {
            act(branch, Action::ChooseEffectTarget(Target::Object(other)));
            assert_eq!(branch.players[player.index()].graveyard, [other, donor]);
            let markers = branch
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter_map(|(index, event)| {
                    matches!(event, CommittedEvent::FullTextBatchEntryV1 { .. })
                        .then_some(index as u64)
                })
                .collect::<Vec<_>>();
            let [marker] = markers.as_slice() else {
                panic!("one real mixed entry marker")
            };
            let (entry, text) = batch_entry(branch, *marker).unwrap();
            assert!(matches!(branch.engine.event_history[entry as usize],
                CommittedEvent::ZoneChange { object, from: Zone::Hand, to: Zone::Battlefield, .. }
                    if object == source));
            assert_eq!(
                text.host(),
                AbilitySourceContractV4::capture(branch, source)
            );
            assert_eq!(
                text.donor(),
                ObjectLinkV4 {
                    object: donor,
                    zone_change_count: branch.objects.get(donor).zone_change_count,
                }
            );
            let (captured_at, _) = branch
                .engine
                .event_history
                .iter()
                .enumerate()
                .find(|(_, event)| {
                    matches!(event, CommittedEvent::FullTextTriggerCapturedV1 { .. })
                })
                .unwrap();
            let capture = trigger_record(branch, captured_at as u64).unwrap();
            assert_eq!(
                capture.kind,
                DonatedTriggerKindV1::PalinchronEnteredAfterBatch {
                    entry,
                    marker: *marker
                }
            );
            assert_eq!(capture.program(branch).unwrap(), p2c::palinchron_enters());
            let mut forged = branch.clone();
            let CommittedEvent::FullTextBatchEntryV1 {
                donor: recorded, ..
            } = &mut forged.engine.event_history[*marker as usize]
            else {
                unreachable!("the genuine marker was checked above");
            };
            recorded.object = other;
            assert!(trigger_record(&forged, captured_at as u64).is_err());
            resolve_untap(branch, player, source, land);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn volrath_native_cast_captures_and_pays_for_palinchron_text_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        let donor = put(&mut state, player, "Palinchron", Zone::Graveyard);
        let land = put(&mut state, player, "Island", Zone::Battlefield);
        state.objects.get_mut(land).tapped = true;
        let source = cast_volrath(&mut state, player);
        resolve_untap(&mut state, player, source, land);
        let (captured_at, _) = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .find(|(_, event)| matches!(event, CommittedEvent::FullTextTriggerCapturedV1 { .. }))
            .unwrap();
        let capture = trigger_record(&state, captured_at as u64).unwrap();
        assert!(matches!(
            capture.kind,
            DonatedTriggerKindV1::PalinchronEntered { .. }
        ));
        assert_eq!(capture.host.card_def, id(VOLRATH));
        assert_eq!(capture.donor.source, donor);
        assert_eq!(state.objects.get(source).card_def, id(VOLRATH));
        assert_eq!(engine::effective_power(&state, source), 4);

        pool(&mut state, player, &[ManaColor::U, ManaColor::U], 2);
        let Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } = next(&mut state)
        else {
            panic!("expected the donated activation");
        };
        let donated = activatable_abilities
            .iter()
            .filter_map(|&(object, index)| (object == source && index != 0).then_some(index))
            .collect::<Vec<_>>();
        let [index] = donated.as_slice() else {
            panic!("one native donated ability")
        };
        act(&mut state, Action::ActivateAbility(source, *index));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        let item = state.stack.last().unwrap();
        let producer = item.v4.full_text_activation_v1.0.unwrap();
        assert_eq!(producer.host.card_def, id(VOLRATH));
        assert_eq!(producer.donor.card_def, id("Palinchron"));
        assert_eq!(producer.donor.source, donor);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
        assert!(active_text(&state, source).is_none());
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            settled(branch);
            assert_eq!(branch.objects.get(source).zone, Zone::Hand);
            assert_eq!(branch.objects.get(source).card_def, id(VOLRATH));
            assert_eq!(branch.objects.get(donor).zone, Zone::Exile);
        }
        assert_eq!(state.state_hash(), restored.state_hash());
    }
}

#[test]
fn volrath_native_paid_fanatic_uses_original_damage_source_both_seats() {
    use crate::fixed_source_damage_v3;
    use crate::premodern_ordered_public_v1::{observe, DonatedActivationV1, FullTextProgramV1};

    for player in [PlayerId::P0, PlayerId::P1] {
        let opponent = player.opponent();
        for (protect, prevent) in [(false, false), (true, false), (false, true)] {
            let mut state = ordered_game(player);
            let donor = put(&mut state, player, "Mogg Fanatic", Zone::Graveyard);
            let target = if protect {
                Target::Object(put(
                    &mut state,
                    opponent,
                    "Tireless Tribe",
                    Zone::Battlefield,
                ))
            } else {
                Target::Player(opponent)
            };
            let mother =
                protect.then(|| put(&mut state, opponent, "Mother of Runes", Zone::Battlefield));
            let strands =
                prevent.then(|| put(&mut state, opponent, "Prismatic Strands", Zone::Hand));
            let source = cast_volrath(&mut state, player);
            settled(&mut state);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
            let host = AbilitySourceContractV4::capture(&state, source);
            let original_color = engine::object_color_mask(&state, source);
            assert_eq!(
                original_color,
                crate::card_def::mana_color_mask(ManaColor::R)
            );
            let Decision::CastSpellOrPass {
                activatable_abilities,
                ..
            } = next(&mut state)
            else {
                panic!("expected the paid Shapeshifter's native menu");
            };
            let donated = activatable_abilities
                .into_iter()
                .filter_map(|(object, index)| (object == source && index != 0).then_some(index))
                .collect::<Vec<_>>();
            let [index] = donated.as_slice() else {
                panic!("expected one donated activation")
            };
            act(&mut state, Action::ActivateAbility(source, *index));
            assert!(
                matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
                if legal_targets.contains(&target))
            );
            assert!(observe(&state, player)
                .unwrap()
                .producers
                .iter()
                .any(|row| {
                    row.native_ability_index == Some(*index)
                        && row.program
                            == (FullTextProgramV1::DonatedActivation {
                                printed_index: 0,
                                effect: DonatedActivationV1::MoggFanaticSacrificeForDamage,
                            })
                }));
            act(&mut state, Action::ChooseTarget(target));
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            let item = state.stack.last().unwrap().clone();
            assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(state.objects.get(donor).zone, Zone::Graveyard);
            assert_eq!(item.v4.ability_source_contract, Some(host));
            assert_eq!(
                item.v4.full_text_activation_v1.0.unwrap().donor.source,
                donor
            );
            assert_eq!(
                fixed_source_damage_v3::damage_source(&state, source, item.v4.stack_item_id)
                    .unwrap()
                    .color_mask,
                original_color,
            );
            let mut forged = state.clone();
            forged.stack.last_mut().unwrap().v4.fixed_damage_paid_v3 = Default::default();
            assert!(fixed_source_damage_v3::validate_state(&forged).is_err());
            let ctx = crate::effect::crystal_prevention_v1::context(&item);
            let mut forged = state.clone();
            crate::effect::execute(&fixed_source_damage_v3::printed_root(), &ctx, &mut forged);
            assert!(forged.engine.halted.is_some());
            assert_eq!(forged.engine.event_history, state.engine.event_history);

            let mut forged_damage = event::ProposedEvent::damage(source, target, 1);
            let event::ProposedEvent::Damage(damage) = &mut forged_damage else {
                unreachable!();
            };
            damage.historical_source = Some(item.v4.stack_item_id);
            for apply in [event::propose_and_commit, event::commit] {
                let mut forged = state.clone();
                apply(&mut forged, forged_damage.clone());
                assert!(forged.engine.halted.is_some());
                assert_eq!(forged.engine.event_history, state.engine.event_history);
            }

            // A later incarnation now has the original blue physical text.
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(donor, Zone::Exile));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert_ne!(engine::object_color_mask(&state, source), original_color);
            assert!(state.objects.get(source).zone_change_count > host.zone_change_count);
            let surface = crate::surface_v2::HarnessSurfaceV2::new();
            for viewer in [player, opponent] {
                crate::rl::observe_premodern_ordered_v1(&state, &surface, viewer, 2).unwrap();
            }
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                if let Some(mother) = mother {
                    act(branch, Action::Pass);
                    assert!(matches!(next(branch), Decision::CastSpellOrPass {
                        player: chooser, activatable_abilities, ..
                    } if chooser == opponent && activatable_abilities.contains(&(mother, 0))));
                    act(branch, Action::ActivateAbility(mother, 0));
                    assert!(
                        matches!(next(branch), Decision::ChooseTargets { legal_targets, .. }
                        if legal_targets.contains(&target))
                    );
                    act(branch, Action::ChooseTarget(target));
                    assert!(matches!(
                        settle(branch),
                        Some(Decision::ChooseEffectOption { .. })
                    ));
                    act(branch, Action::ChooseEffectOption(3)); // red in WUBRG order
                    assert!(branch.objects.get(mother).tapped);
                }
                if let Some(strands) = strands {
                    act(branch, Action::Pass);
                    pool(branch, opponent, &[ManaColor::W], 2);
                    cast(branch, strands);
                    assert!(matches!(
                        settle(branch),
                        Some(Decision::ChooseEffectOption { player, source, option_count: 5 })
                        if player == opponent && source == strands
                    ));
                    act(branch, Action::ChooseEffectOption(3)); // red in WUBRG order
                }
                let deals_damage = !protect && !prevent;
                let life_before = branch.players[opponent.index()].life;
                settled(branch);
                assert_eq!(
                    branch.players[opponent.index()].life,
                    life_before - i32::from(deals_damage)
                );
                assert_eq!(
                    branch
                        .engine
                        .event_history
                        .iter()
                        .filter(|event| {
                            matches!(event, CommittedEvent::Damage {
                        source: actual, target: recipient, amount: 1,
                    } if *actual == source && *recipient == target)
                        })
                        .count(),
                    usize::from(deals_damage)
                );
                let outcome = if protect {
                    fixed_source_damage_v3::OutcomeV3::NoLegalTarget
                } else {
                    fixed_source_damage_v3::OutcomeV3::Resolved
                };
                assert!(branch.engine.event_history.iter().any(|event| {
                    matches!(event, CommittedEvent::FixedDamageRemovedV3 { removal }
                        if removal.item.v4.stack_item_id == item.v4.stack_item_id
                            && removal.outcome == outcome)
                }));
                fixed_source_damage_v3::validate_state(branch).unwrap();
                crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                for viewer in [player, opponent] {
                    crate::rl::observe_premodern_ordered_v1(branch, &surface, viewer, 3).unwrap();
                }

                if let Some(strands) = strands {
                    use crate::effect::crystal_prevention_v1::PreventionV1;
                    use crate::event::ReplacementEffectKind;

                    assert_eq!(branch.objects.get(strands).zone, Zone::Graveyard);
                    assert_eq!(branch.players[opponent.index()].mana_pool, [0; 6]);
                    let mut forged: GameState =
                        serde_json::from_slice(&serde_json::to_vec(branch).unwrap()).unwrap();
                    let operation = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find_map(|event| match event {
                            CommittedEvent::CrystalDamageOperationV3 { operation }
                                if operation.historical_source == Some(item.v4.stack_item_id) =>
                            {
                                Some(operation)
                            }
                            _ => None,
                        })
                        .expect("paid Fanatic has a native damage operation");
                    assert!(operation.output.is_none() && operation.damage_history.is_none());
                    assert!(matches!(
                        operation.preventers.as_slice(),
                        [PreventionV1::Replacement(replacement)]
                            if replacement.source == strands
                                && matches!(
                                    replacement.kind,
                                    ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {
                                        color: ManaColor::R, ..
                                    }
                                )
                    ));
                    operation.preventers.clear();
                    assert_eq!(
                        crate::effect::crystal_prevention_v1::validate(&forged).unwrap_err(),
                        "Crystal damage outcome lost actual positive or fully-prevented result"
                    );
                }
                if protect {
                    let mut forged: GameState =
                        serde_json::from_slice(&serde_json::to_vec(branch).unwrap()).unwrap();
                    let removal = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find_map(|event| match event {
                            CommittedEvent::FixedDamageRemovedV3 { removal }
                                if removal.item.v4.stack_item_id == item.v4.stack_item_id =>
                            {
                                Some(removal)
                            }
                            _ => None,
                        })
                        .unwrap();
                    removal.outcome = fixed_source_damage_v3::OutcomeV3::Resolved;
                    assert!(fixed_source_damage_v3::validate_state(&forged).is_err());
                }
            }
            assert_eq!(state.state_hash(), restored.state_hash());
        }
    }
}

fn resolve_rats_discard(
    state: &mut GameState,
    player: PlayerId,
    source: ObjectId,
    hand: [ObjectId; 2],
) {
    let opponent = player.opponent();
    let target = Target::Player(opponent);
    let decision = settle(state);
    let decision = if let Some(Decision::ChooseTargets {
        player: chooser,
        spell,
        legal_targets,
        ..
    }) = &decision
    {
        assert_eq!((*chooser, *spell), (player, source));
        assert_eq!(legal_targets.as_slice(), &[target]);
        act(state, Action::ChooseTarget(target));
        settle(state)
    } else {
        decision
    };
    assert!(
        matches!(&decision, Some(Decision::Discard { player: chooser, count: 1, choices })
        if *chooser == opponent && choices.as_slice() == hand)
    );
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(Some(next(&mut restored)), decision);
    for branch in [&mut *state, &mut restored] {
        act(branch, Action::Discard(vec![hand[1]]));
        settled(branch);
        assert_eq!(branch.players[opponent.index()].hand, [hand[0]]);
        assert_eq!(branch.players[opponent.index()].graveyard, [hand[1]]);
    }
    assert_eq!(state.state_hash(), restored.state_hash());
}

#[test]
fn volrath_native_paid_rats_discards_for_the_opponent_unless_suppressed_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for suppressed in [false, true] {
            let mut state = ordered_game(player);
            let donor = put(&mut state, player, "Ravenous Rats", Zone::Graveyard);
            let hand = [
                put(&mut state, player.opponent(), "Forest", Zone::Hand),
                put(&mut state, player.opponent(), "Island", Zone::Hand),
            ];
            if suppressed {
                let orb = put(&mut state, player, "Torpor Orb", Zone::Hand);
                pool(&mut state, player, &[], 2);
                cast(&mut state, orb);
                settled(&mut state);
                assert_eq!(state.objects.get(orb).zone, Zone::Battlefield);
            }
            let source = cast_volrath(&mut state, player);
            if suppressed {
                settled(&mut state);
                assert!(captured_entry(&state, source).is_none());
                assert_eq!(state.players[player.opponent().index()].hand, hand);
            } else {
                resolve_rats_discard(&mut state, player, source, hand);
                let capture = captured_entry(&state, source).unwrap();
                assert_eq!(capture.donor.source, donor);
                assert!(matches!(
                    capture.kind,
                    DonatedTriggerKindV1::RavenousRatsEntered { .. }
                ));
            }
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        }
    }
}

#[test]
fn volrath_native_rats_uses_final_batch_text_after_donor_departure_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for donor_first in [false, true] {
            let mut state = ordered_game(player);
            let source = put(&mut state, player, VOLRATH, Zone::Hand);
            let donor = put(&mut state, player, "Ravenous Rats", Zone::Graveyard);
            let departed = put(&mut state, player, "Palinchron", Zone::Graveyard);
            let hand = [
                put(&mut state, player.opponent(), "Forest", Zone::Hand),
                put(&mut state, player.opponent(), "Island", Zone::Hand),
            ];
            let mut changes = vec![
                ProposedEvent::zone_change(source, Zone::Battlefield),
                ProposedEvent::zone_change(departed, Zone::Exile),
            ];
            if donor_first {
                changes.reverse();
            }
            event::propose_and_commit_batch(&mut state, changes);
            let capture = captured_entry(&state, source).unwrap();
            assert_eq!(capture.donor.source, donor);
            assert!(matches!(
                capture.kind,
                DonatedTriggerKindV1::RavenousRatsEnteredAfterBatch { .. }
            ));
            assert_eq!(state.objects.get(departed).zone, Zone::Exile);
            resolve_rats_discard(&mut state, player, source, hand);
        }
    }
}

#[test]
fn volrath_native_wall_mana_records_the_exact_restricted_use_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        let donor = put(&mut state, player, "Wall of Roots", Zone::Graveyard);
        let source = cast_volrath(&mut state, player);
        settled(&mut state);
        let text = active_text(&state, source).unwrap();
        assert!(!text.wall_mana_used(&state));
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { mana_abilities, .. }
            if mana_abilities.contains(&source))
        );
        act(&mut state, Action::ActivateManaAbility(source));
        let used = restricted_use_event(&state, text.host(), text.donor(), AbilityKindV4::Mana, 0);
        assert_eq!(text.donor().object, donor);
        assert_eq!(
            state
                .engine
                .event_history
                .iter()
                .filter(|event| **event == used)
                .count(),
            1
        );
        assert_eq!(
            state.players[player.index()].mana_pool[ManaColor::G.pool_index()],
            1
        );
        assert_eq!(engine::effective_toughness(&state, source), 4);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(text.wall_mana_used(&restored));
        assert!(
            matches!(next(&mut restored), Decision::CastSpellOrPass { mana_abilities, .. }
            if !mana_abilities.contains(&source))
        );
    }
}

#[test]
fn volrath_native_paid_control_transfer_records_the_new_controller_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let controller = player.opponent();
        let mut state = ordered_game(player);
        let donor = put(&mut state, player, "Phyrexian Devourer", Zone::Graveyard);
        // A supported donor: an unsupported one halts the transfer (see
        // `volrath_control_change_onto_an_unsupported_top_halts_both_seats`).
        let new_donor = put(&mut state, controller, "Birds of Paradise", Zone::Graveyard);
        let source = cast_volrath(&mut state, player);
        settled(&mut state);
        assert_eq!(active_text(&state, source).unwrap().donor().object, donor);
        assert!(engine::object_has_type(&state, source, CardType::Artifact));
        let link = ObjectLinkV4 {
            object: source,
            zone_change_count: state.objects.get(source).zone_change_count,
        };

        state.active_player = controller;
        state.priority_player = controller;
        let dack = put(&mut state, controller, "Dack Fayden", Zone::Hand);
        pool(&mut state, controller, &[ManaColor::U, ManaColor::R], 1);
        cast(&mut state, dack);
        settled(&mut state);
        act(&mut state, Action::ActivateAbility(dack, 1));
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(source)))
        );
        act(&mut state, Action::ChooseTarget(Target::Object(source)));
        settled(&mut state);
        assert_eq!(crate::planeswalker_v1::loyalty(&state, dack), Some(1));
        assert_eq!(state.players[controller.index()].mana_pool, [0; 6]);
        assert!(state.engine.event_history.iter().any(|event| {
            matches!(event, CommittedEvent::OrderedGraveyardControlGainedV1 { permanent, controller: actor }
                if *permanent == link && *actor == controller)
        }));
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.objects.get(source).owner, player);
        assert_eq!(restored.objects.get(source).controller, controller);
        assert_eq!(
            restored.objects.get(source).zone_change_count,
            link.zone_change_count
        );
        assert_eq!(
            active_text(&restored, source).unwrap().donor().object,
            new_donor
        );
    }
}

// Donor scope (docs/design/premodern_shapeshifter_alter_reality_v1.md,
// sections 2.2, 2.3 and 5.1).

/// Refused donors: a sample of appendix A's refused class, among them the
/// cards whose programs are only partly donated, and the listed additions
/// whose programs are still pending.
const REFUSED_SAMPLE: [&str; 7] = [
    "Masticore",
    "Phantom Nishoba",
    "Spike Feeder",
    "Exalted Angel",
    "Tireless Tribe",
    "Ghitu Slinger",
    "Xantid Swarm",
];

/// Supported cards whose enters ability a Shapeshifter entering with one on
/// top would lose (written out in the test module, not taken from the
/// production lists).
fn entry_refused_cases() -> Vec<&'static str> {
    let mut names = ENTRY_TRIGGER_FIXTURE_DONORS.to_vec();
    names.push("Meddling Mage");
    names
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn donor_halt(host: ObjectId) -> Option<(UnsupportedMechanic, ObjectId)> {
    Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, host))
}

/// The unsupported top already exists: the first decision must be the typed
/// halt, and the game stays halted, also after a snapshot restore.
fn assert_halted_now(state: &mut GameState, host: ObjectId) {
    let halted = Decision::Halted {
        mechanic: UnsupportedMechanic::FullTextDonorOutsideSetV1,
        source: host,
    };
    assert_eq!(engine::advance_until_decision(state), halted);
    assert_eq!(state.engine.halted, donor_halt(host));
    let mut copy = restored(state);
    for branch in [&mut *state, &mut copy] {
        for _ in 0..2 {
            assert_eq!(engine::advance_until_decision(branch), halted);
        }
    }
    assert_eq!(state.state_hash(), copy.state_hash());
}

/// Passes priority until the cast Shapeshifter has entered the battlefield.
fn resolve_to_entry(state: &mut GameState, source: ObjectId) {
    for _ in 0..8 {
        if state.objects.get(source).zone == Zone::Battlefield {
            return;
        }
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        act(state, Action::Pass);
    }
    panic!("the Shapeshifter did not resolve");
}

fn torpor_orb_in_play(state: &mut GameState, player: PlayerId) -> ObjectId {
    let orb = put(state, player, "Torpor Orb", Zone::Hand);
    pool(state, player, &[], 2);
    cast(state, orb);
    settled(state);
    assert_eq!(state.objects.get(orb).zone, Zone::Battlefield);
    orb
}

/// A Shapeshifter on the battlefield whose top is Birds of Paradise.
fn volrath_in_play(player: PlayerId) -> (GameState, ObjectId) {
    let mut state = ordered_game(player);
    put(&mut state, player, "Birds of Paradise", Zone::Graveyard);
    let source = cast_volrath(&mut state, player);
    settled(&mut state);
    assert_eq!(
        active_definition(&state, source).unwrap().name,
        "Birds of Paradise"
    );
    (state, source)
}

fn arrive(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(card, Zone::Graveyard));
    card
}

fn only_retained_discard(state: &mut GameState, player: PlayerId, source: ObjectId) {
    pool(state, player, &[], 2);
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = next(state)
    else {
        panic!("expected native activation choices");
    };
    assert_eq!(
        activatable_abilities
            .iter()
            .copied()
            .filter(|(object, _)| *object == source)
            .collect::<Vec<_>>(),
        [(source, 0)],
    );
}

#[test]
fn volrath_entry_with_an_unsupported_or_entry_only_top_halts_at_the_entry_both_seats() {
    let donors = REFUSED_SAMPLE.into_iter().chain(entry_refused_cases());
    for donor in donors {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ordered_game(player);
            put(&mut state, player, donor, Zone::Graveyard);
            let source = cast_volrath(&mut state, player);
            resolve_to_entry(&mut state, source);
            assert_eq!(state.engine.halted, donor_halt(source), "{donor}");
            assert_halted_now(&mut state, source);
            assert!(captured_entry(&state, source).is_none(), "{donor}");
        }
    }
}

/// An entry-trigger suppressor means the lost enters trigger would not have
/// triggered anyway, so the entry plays correctly and is not refused.
#[test]
fn volrath_entering_under_torpor_orb_with_an_entry_trigger_top_is_not_refused_both_seats() {
    for donor in ENTRY_TRIGGER_FIXTURE_DONORS {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ordered_game(player);
            torpor_orb_in_play(&mut state, player);
            put(&mut state, player, donor, Zone::Graveyard);
            let hand = state.players[player.index()].hand.len();
            let battlefield = state.players[player.index()].battlefield.len();
            let source = cast_volrath(&mut state, player);
            settled(&mut state);
            assert_eq!(state.engine.halted, None, "{donor}");
            assert_eq!(active_definition(&state, source).unwrap().name, donor);
            assert_eq!(state.players[player.index()].hand.len(), hand);
            assert_eq!(
                state.players[player.index()].battlefield.len(),
                battlefield + 1
            );
            let mut copy = restored(&state);
            assert!(matches!(next(&mut copy), Decision::CastSpellOrPass { .. }));
        }
    }
}

/// Meddling Mage's naming is a replacement effect as the permanent enters
/// (614.12), not a trigger, so an entry-trigger suppressor does not cover it.
#[test]
fn volrath_entering_under_torpor_orb_with_meddling_mage_on_top_is_still_refused_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        torpor_orb_in_play(&mut state, player);
        put(&mut state, player, "Meddling Mage", Zone::Graveyard);
        let source = cast_volrath(&mut state, player);
        resolve_to_entry(&mut state, source);
        assert_eq!(state.engine.halted, donor_halt(source));
        assert_halted_now(&mut state, source);
    }
}

/// Suppression is judged on the final simultaneous-entry scene: an Orb that
/// enters with the Shapeshifter suppresses its lost trigger, and one that
/// leaves in the same batch does not.
#[test]
fn volrath_simultaneous_entry_refusal_obeys_the_final_suppressor_scene_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for arriving in [true, false] {
            for orb_first in [true, false] {
                let mut state = ordered_game(player);
                let source = put(&mut state, player, VOLRATH, Zone::Hand);
                put(&mut state, player, "Wall of Blossoms", Zone::Graveyard);
                let (from, to) = if arriving {
                    (Zone::Hand, Zone::Battlefield)
                } else {
                    (Zone::Battlefield, Zone::Exile)
                };
                let orb = put(&mut state, player.opponent(), "Torpor Orb", from);
                let hand = state.players[player.index()].hand.len();
                let mut changes = vec![
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                    ProposedEvent::zone_change(orb, to),
                ];
                if orb_first {
                    changes.reverse();
                }
                event::propose_and_commit_batch(&mut state, changes);
                assert_eq!(state.objects.get(orb).zone, to);
                assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                if arriving {
                    assert_eq!(state.engine.halted, None);
                    settled(&mut state);
                    assert_eq!(state.players[player.index()].hand.len(), hand - 1);
                } else {
                    assert_eq!(state.engine.halted, donor_halt(source));
                    assert_halted_now(&mut state, source);
                }
            }
        }
    }
}

/// A state-based death in the engine's automatic work makes an unsupported
/// card the top, here together with a loss: the first decision that leaves
/// the engine is the typed halt, not a priority prompt or a terminal.
#[test]
fn volrath_state_based_death_making_an_unsupported_top_halts_at_the_first_advance_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for loss in [false, true] {
            let (mut state, source) = volrath_in_play(player);
            let masticore = put(&mut state, player, "Masticore", Zone::Battlefield);
            state.objects.get_mut(masticore).damage = 4;
            if loss {
                state.players[player.opponent().index()].life = 0;
            }
            assert_eq!(state.engine.halted, None);
            assert_halted_now(&mut state, source);
            assert_eq!(state.objects.get(masticore).zone, Zone::Graveyard);
        }
    }
}

#[test]
fn volrath_returning_from_the_graveyard_onto_a_refused_top_halts_at_the_entry_both_seats() {
    for donor in ["Wall of Blossoms", "Masticore"] {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ordered_game(player);
            put(&mut state, player, donor, Zone::Graveyard);
            let source = put(&mut state, player, VOLRATH, Zone::Graveyard);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert_eq!(
                state.engine.halted,
                Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, source))
            );
            assert_halted_now(&mut state, source);
        }
    }
}

#[test]
fn volrath_entering_with_a_generic_top_is_not_refused_both_seats() {
    for donor in GENERIC_FIXTURE_DONORS {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ordered_game(player);
            put(&mut state, player, donor, Zone::Graveyard);
            let source = cast_volrath(&mut state, player);
            settled(&mut state);
            assert_eq!(state.engine.halted, None);
            assert_eq!(active_definition(&state, source).unwrap().name, donor);
            assert!(captured_entry(&state, source).is_none(), "{donor}");
            assert!(state.stack.is_empty());
        }
    }
}

#[test]
fn volrath_in_play_takes_generic_and_entry_only_tops_with_their_keywords_both_seats() {
    use crate::card_def::Keywords;

    let donors = GENERIC_FIXTURE_DONORS
        .into_iter()
        .chain(entry_refused_cases());
    for donor in donors {
        for player in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = volrath_in_play(player);
            arrive(&mut state, player, donor);
            assert_eq!(state.engine.halted, None, "{donor}");
            let text = active_definition(&state, source).unwrap();
            assert_eq!(text.name, donor);
            assert_eq!(
                engine::effective_keywords(&state, source, Keywords(u32::MAX)),
                text.keywords,
                "{donor}"
            );
            only_retained_discard(&mut state, player, source);
            let mut copy = restored(&state);
            assert!(matches!(next(&mut copy), Decision::CastSpellOrPass { .. }));
        }
    }
}

#[test]
fn volrath_single_arrival_of_an_unsupported_donor_halts_at_the_event_both_seats() {
    for donor in REFUSED_SAMPLE {
        for player in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = volrath_in_play(player);
            arrive(&mut state, player, donor);
            assert_eq!(
                state.engine.halted,
                Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, source)),
                "{donor}"
            );
            assert_halted_now(&mut state, source);
        }
    }
}

#[test]
fn volrath_departure_exposing_an_unsupported_card_halts_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        put(&mut state, player, "Masticore", Zone::Graveyard);
        let birds = put(&mut state, player, "Birds of Paradise", Zone::Graveyard);
        let source = cast_volrath(&mut state, player);
        settled(&mut state);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(birds, Zone::Exile));
        assert_eq!(
            state.engine.halted,
            Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, source))
        );
        assert_halted_now(&mut state, source);
    }
}

#[test]
fn volrath_control_change_onto_an_unsupported_top_halts_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for (donor, refused) in [("Wall of Roots", false), ("Masticore", true)] {
            let (mut state, source) = volrath_in_play(player);
            let opponent = player.opponent();
            put(&mut state, opponent, donor, Zone::Graveyard);
            crate::standard_cards_v1::gain_control(&mut state, source, opponent);
            assert_eq!(state.objects.get(source).controller, opponent);
            if refused {
                assert_eq!(
                    state.engine.halted,
                    Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, source))
                );
                assert_halted_now(&mut state, source);
            } else {
                assert_eq!(state.engine.halted, None);
                assert_eq!(active_definition(&state, source).unwrap().name, donor);
                assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            }
        }
    }
}

#[test]
fn volrath_phasing_in_onto_an_unsupported_top_halts_both_seats() {
    use crate::eternal_e5a_v1::phasing;

    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = volrath_in_play(player);
        phasing::phase_out(&mut state, source);
        assert!(phasing::phased_out(&state, source));
        // Phased out, it is treated as though it does not exist.
        arrive(&mut state, player, "Masticore");
        assert_eq!(state.engine.halted, None);
        phasing::phase_in(&mut state, player);
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert_eq!(
            state.engine.halted,
            Some((UnsupportedMechanic::FullTextDonorOutsideSetV1, source))
        );
        assert_halted_now(&mut state, source);
    }
}

/// Two library cards arrive in the graveyard at once under the ordered
/// profile, so the owner orders them. The order menu is offered, also after a
/// restore without a runner, and the halt fires only once the order makes the
/// unsupported donor the top.
#[test]
fn volrath_ordered_batch_offers_the_order_and_halts_only_on_its_final_top_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for donor_on_top in [true, false] {
            let (mut state, source) = volrath_in_play(player);
            let donor = put(&mut state, player, "Masticore", Zone::Library);
            let land = put(&mut state, player, "Island", Zone::Library);
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(donor, Zone::Graveyard),
                    ProposedEvent::zone_change(land, Zone::Graveyard),
                ],
            );
            let prompt = next(&mut state);
            assert!(
                matches!(&prompt, Decision::ChooseEffectTargets { player: chooser, legal_targets, .. }
                if *chooser == player && legal_targets.contains(&Target::Object(donor)))
            );
            assert_eq!(state.engine.halted, None);
            let mut copy = restored(&state);
            assert_eq!(next(&mut copy), prompt);
            // The first pick goes to the bottom.
            let bottom = if donor_on_top { land } else { donor };
            for branch in [&mut state, &mut copy] {
                act(branch, Action::ChooseEffectTarget(Target::Object(bottom)));
                if donor_on_top {
                    // Halted by the batch tail, once the order selects the top.
                    assert_eq!(branch.engine.halted, donor_halt(source));
                    assert_halted_now(branch, source);
                } else {
                    assert_eq!(branch.engine.halted, None);
                    assert!(active_text(branch, source).is_none());
                    assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                }
            }
            assert_eq!(state.state_hash(), copy.state_hash());
        }
    }
}

/// A Shapeshifter entering in the same batch as two graveyard arrivals takes
/// the batch's final top as its text at entry.
#[test]
fn volrath_entering_in_an_ordered_batch_halts_only_when_its_final_top_is_entry_only_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for donor_on_top in [true, false] {
            let mut state = ordered_game(player);
            let source = put(&mut state, player, VOLRATH, Zone::Hand);
            let donor = put(&mut state, player, "Wall of Blossoms", Zone::Library);
            let land = put(&mut state, player, "Island", Zone::Library);
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                    ProposedEvent::zone_change(donor, Zone::Graveyard),
                    ProposedEvent::zone_change(land, Zone::Graveyard),
                ],
            );
            let prompt = next(&mut state);
            assert!(matches!(&prompt, Decision::ChooseEffectTargets { .. }));
            assert_eq!(state.engine.halted, None);
            let mut copy = restored(&state);
            assert_eq!(next(&mut copy), prompt);
            let bottom = if donor_on_top { land } else { donor };
            for branch in [&mut state, &mut copy] {
                act(branch, Action::ChooseEffectTarget(Target::Object(bottom)));
                assert_eq!(branch.objects.get(source).zone, Zone::Battlefield);
                if donor_on_top {
                    // Halted at the entry, with the batch's final top.
                    assert_eq!(branch.engine.halted, donor_halt(source));
                    assert_halted_now(branch, source);
                } else {
                    settled(branch);
                    assert!(active_text(branch, source).is_none());
                }
            }
            assert_eq!(state.state_hash(), copy.state_hash());
        }
    }
}

#[test]
fn volrath_with_ihsans_shade_or_iridescent_angel_text_has_their_protection_both_seats() {
    let white = crate::card_def::mana_color_mask(ManaColor::W);
    // White, blue, black, red and green in the schema-v4 bit order.
    let each: u8 = 0b1_1111;
    for (donor, protection) in [("Ihsan's Shade", white), ("Iridescent Angel", each)] {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ordered_game(player);
            put(&mut state, player, donor, Zone::Graveyard);
            let source = cast_volrath(&mut state, player);
            settled(&mut state);
            assert_eq!(state.engine.halted, None);
            assert_eq!(
                crate::eternal_legacy_e9a_v1::statics::protection_mask(&state, source),
                protection,
                "{donor}"
            );
            // The physical card would have no protection without the text.
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(
                crate::eternal_legacy_e9a_v1::statics::protection_mask(&state, source),
                0
            );
        }
    }
}

#[test]
fn volrath_with_akroma_text_and_the_physical_akroma_meet_the_legend_rule_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ordered_game(player);
        put(
            &mut state,
            player,
            "Akroma, Angel of Wrath",
            Zone::Graveyard,
        );
        let source = cast_volrath(&mut state, player);
        settled(&mut state);
        assert_eq!(
            engine::effective_name(&state, source),
            "Akroma, Angel of Wrath"
        );
        let akroma = put(&mut state, player, "Akroma, Angel of Wrath", Zone::Hand);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(akroma, Zone::Battlefield),
        );
        assert_eq!(state.engine.halted, None);
        assert!(
            matches!(next(&mut state), Decision::ChooseLegendPermanent { player: chooser, candidates }
            if chooser == player && candidates.contains(&source) && candidates.contains(&akroma))
        );
        act(&mut state, Action::ChooseLegendPermanent(akroma));
        settled(&mut state);
        assert_eq!(state.objects.get(akroma).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
    }
}

/// One case per check site. Each returns whether that site's contract held:
/// the event sites halt at the event itself, before any decision; the
/// backstops halt at the first decision, the start one before any automatic
/// work. Each case is built so that no other site catches it in time.
fn check_site_case(site: DonorCheckSiteV1, player: PlayerId) -> bool {
    use crate::eternal_e5a_v1::phasing;

    match site {
        DonorCheckSiteV1::CommitTail => {
            let (mut state, source) = volrath_in_play(player);
            arrive(&mut state, player, "Masticore");
            state.engine.halted == donor_halt(source)
        }
        DonorCheckSiteV1::BatchTail => {
            let (mut state, source) = volrath_in_play(player);
            let donor = put(&mut state, player, "Masticore", Zone::Library);
            let land = put(&mut state, player, "Island", Zone::Library);
            event::propose_and_commit_batch(
                &mut state,
                vec![
                    ProposedEvent::zone_change(donor, Zone::Graveyard),
                    ProposedEvent::zone_change(land, Zone::Graveyard),
                ],
            );
            assert!(matches!(
                next(&mut state),
                Decision::ChooseEffectTargets { .. }
            ));
            act(&mut state, Action::ChooseEffectTarget(Target::Object(land)));
            state.engine.halted == donor_halt(source)
        }
        DonorCheckSiteV1::Entry => {
            // Wall of Blossoms is accepted in play, so only the entry refuses.
            let mut state = ordered_game(player);
            put(&mut state, player, "Wall of Blossoms", Zone::Graveyard);
            let source = cast_volrath(&mut state, player);
            resolve_to_entry(&mut state, source);
            state.engine.halted == donor_halt(source)
        }
        DonorCheckSiteV1::ControlChange => {
            let (mut state, source) = volrath_in_play(player);
            put(&mut state, player.opponent(), "Masticore", Zone::Graveyard);
            crate::standard_cards_v1::gain_control(&mut state, source, player.opponent());
            state.engine.halted == donor_halt(source)
        }
        DonorCheckSiteV1::PhaseIn => {
            let (mut state, source) = volrath_in_play(player);
            phasing::phase_out(&mut state, source);
            arrive(&mut state, player, "Masticore");
            phasing::phase_in(&mut state, player);
            state.engine.halted == donor_halt(source)
        }
        DonorCheckSiteV1::AdvanceStart => {
            // The top changes with no event; a lethally damaged creature
            // shows whether automatic work ran before the refusal.
            let (mut state, source) = volrath_in_play(player);
            put(&mut state, player, "Masticore", Zone::Graveyard);
            let dying = put(
                &mut state,
                player.opponent(),
                "Tireless Tribe",
                Zone::Battlefield,
            );
            state.objects.get_mut(dying).damage = 100;
            let halted = engine::advance_until_decision(&mut state)
                == Decision::Halted {
                    mechanic: UnsupportedMechanic::FullTextDonorOutsideSetV1,
                    source,
                };
            halted && state.objects.get(dying).zone == Zone::Battlefield
        }
        DonorCheckSiteV1::AdvanceDecision => {
            // A state-based death inside the automatic work makes the top.
            let (mut state, source) = volrath_in_play(player);
            let masticore = put(&mut state, player, "Masticore", Zone::Battlefield);
            state.objects.get_mut(masticore).damage = 4;
            engine::advance_until_decision(&mut state)
                == Decision::Halted {
                    mechanic: UnsupportedMechanic::FullTextDonorOutsideSetV1,
                    source,
                }
        }
    }
}

#[test]
fn each_donor_check_site_halts_its_own_case_both_seats() {
    for site in DonorCheckSiteV1::ALL {
        for player in [PlayerId::P0, PlayerId::P1] {
            assert!(check_site_case(site, player), "{site:?} {player:?}");
        }
    }
}

/// With only one site's check removed, that site's case no longer holds: no
/// other site covers it, so every site is needed.
#[test]
fn each_donor_check_site_case_fails_without_that_site_both_seats() {
    for site in DonorCheckSiteV1::ALL {
        for player in [PlayerId::P0, PlayerId::P1] {
            let held = with_donor_check_site_disabled(site, || check_site_case(site, player));
            assert!(!held, "{site:?} {player:?} held without its own check");
        }
    }
}
