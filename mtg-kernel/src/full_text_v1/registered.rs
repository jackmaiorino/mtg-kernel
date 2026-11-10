//! Native fixtures using the registered physical Shapeshifter.

use super::*;
use crate::engine::{self, Action, Decision, RuntimeRulesV1};
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
        for donor_name in [None, Some(VOLRATH), Some("Abyssal Harvester")] {
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
            if donor_name != Some("Abyssal Harvester") {
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
            let strands = prevent.then(|| put(&mut state, opponent, "Prismatic Strands", Zone::Hand));
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

                if prevent {
                    assert_eq!(branch.objects.get(strands.unwrap()).zone, Zone::Graveyard);
                    assert_eq!(branch.players[opponent.index()].mana_pool, [0; 6]);
                    assert!(branch.engine.event_history.iter().any(|event| {
                        matches!(event, CommittedEvent::CrystalDamageOperationV3 { operation }
                            if operation.historical_source == Some(item.v4.stack_item_id)
                                && operation.output.is_none()
                                && operation.damage_history.is_none())
                    }));
                }
                if protect {
                    let mut forged: GameState = serde_json::from_slice(
                        &serde_json::to_vec(branch).unwrap(),
                    ).unwrap();
                    let removal = forged.engine.event_history.iter_mut().find_map(|event| {
                        match event {
                            CommittedEvent::FixedDamageRemovedV3 { removal }
                                if removal.item.v4.stack_item_id == item.v4.stack_item_id =>
                            {
                                Some(removal)
                            }
                            _ => None,
                        }
                    }).unwrap();
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
        if *chooser == opponent && choices.as_slice() == &hand)
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
        let new_donor = put(&mut state, controller, "Tireless Tribe", Zone::Graveyard);
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
