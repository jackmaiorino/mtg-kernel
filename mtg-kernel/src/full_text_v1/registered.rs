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
