//! Focused integration coverage for Lorien Revealed and the reusable
//! hand-zone typecycling/private-search substrate.
//!
//! Oracle baseline: XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`. Relevant pinned blobs and
//! normalized SHA-256 digests are:
//!
//! - `LorienRevealed.java` blob `4bddcb833519fcb8dac2c34c0e32f3299bdf18e0`,
//!   SHA-256 `b5703b792f24f8242c479f5a4ecb5fda128e4ad8f9cd9d3768c9ef29392e0b18`;
//! - `IslandcyclingAbility.java` blob
//!   `719057f498cc3f2d70ed787611d91624683c8add`, SHA-256
//!   `867746f55d6c9731a3fbdbdf7786356a78d0e75d29dc2224c8138169170aa46a`;
//! - `CyclingAbility.java` / `CyclingDiscardCost.java` blobs
//!   `2dd33a99fca177d260f7c8e89601cfff030d18c9` /
//!   `65d621a71fd1de4800cd26de3ae3ad187d459914`, SHA-256
//!   `ef482a6a01ae60a29d658b669d218e5a0c73bc6ef618b2d0c93b287cf5fe753c` /
//!   `1025f6bf0bc3fd16124d317b75d7de4bf3ea880e506a936ff44aa04f9c53c575`;
//! - search/target/filter blobs `bd470534812e8b1341fd4ae6c1f1889094616294`,
//!   `30475de61c237bd74d9de774e7182b5d6e775ce4`, and
//!   `2fe2b9b82acc8b31f64452e7e6e610f7c1fced7f`;
//! - combined oracle digest
//!   `9980f0009ce7615c3560f3481811ff1d640de51c2b98a795923bd92a8b8eccb8`.
//!
//! XMage's rules/human chooser permits fail-to-find, while its frozen AIRL
//! adapter suppresses STOP whenever a match exists and deduplicates candidates
//! by card name. The kernel deliberately implements the rules-correct contract:
//! every matching physical card is a semantic candidate, followed by schema
//! v4's stable Finish action. No candidate-index parity is claimed.

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, CostComponent, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{
    EffectFrame, EffectOp, EffectTargetSelectionPurpose, LibraryCardFilter, PlayerRef,
};
use mtg_kernel::engine::{self, Action, Decision, UnsupportedMechanic};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{
    legal_action_candidates_v1, observe_v2, ActionSemanticV1, PendingEffectChoiceSemanticV4,
    PlayerSeatV1, TargetSelectionPurposeV4,
};
use mtg_kernel::state::{
    Counters, GameObject, GameState, StackItem, StackItemKind, StackStateV4, Step, Target, Zone,
};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceDecision};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn put_object(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
        card_def,
        name: name.to_string(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not create stack-zone objects"),
    }
    id
}

fn ready_inline_search(names: &[&str], seed: u64) -> (GameState, ObjectId, Vec<ObjectId>) {
    let library_defs = names.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(
        &library_defs,
        &[card_id("Snow-Covered Forest")],
        card_name,
        seed,
    );
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    let source = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
    state.stack.push(StackItem {
        kind: StackItemKind::TriggeredAbility,
        source,
        controller: PlayerId::P0,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::LandWithSubtype(Subtype::Island),
        }),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::default(),
    });
    state.engine.priority_passes = [true, true];
    let library = state.players[0].library.clone();
    (state, source, library)
}

fn ready_main(names: &[&str], seed: u64) -> GameState {
    let library_defs = names.iter().map(|name| card_id(name)).collect::<Vec<_>>();
    let mut state = GameState::new_from_libraries(
        &library_defs,
        &[card_id("Snow-Covered Forest")],
        card_name,
        seed,
    );
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn action_candidates(
    state: &GameState,
    decision: &Decision,
) -> Vec<mtg_kernel::rl::LegalActionCandidateV1> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state).unwrap()
}

fn reference_shuffle(
    mut objects: Vec<ObjectId>,
    mut rng: mtg_kernel::state::SplitMix64,
) -> Vec<ObjectId> {
    for i in (1..objects.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        objects.swap(i, j);
    }
    objects
}

#[test]
fn registered_definition_is_draw_three_plus_hand_zone_islandcycling_one() {
    let lorien = &CARD_DEFS[card_id("Lorien Revealed") as usize];
    assert_eq!(lorien.capability, CardCapability::Full);
    assert_eq!(lorien.types, &[CardType::Sorcery]);
    assert_eq!(lorien.target_spec, TargetSpec::None);
    assert_eq!(
        (lorien.spell_effect)(),
        Some(EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        })
    );
    assert_eq!(lorien.activated_abilities.len(), 1);
    let cycling = &lorien.activated_abilities[0];
    assert_eq!(cycling.activation_zone, Zone::Hand);
    assert!(!cycling.sorcery_speed_only);
    assert_eq!(cycling.target_spec, TargetSpec::None);
    assert_eq!(
        cycling.cost,
        &[
            CostComponent::Mana(mtg_kernel::mana::Cost {
                pips: &[],
                generic: 1,
                x_count: 0,
            }),
            CostComponent::DiscardSelf,
        ]
    );
    assert_eq!(
        (cycling.effect)(),
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::LandWithSubtype(Subtype::Island),
        }
    );
}

#[test]
fn activation_stages_then_pays_mana_discards_source_and_uses_the_stack() {
    let mut state = ready_main(&["Island", "Mountain"], 0x4c4f_5249_454e_0001);
    state.active_player = PlayerId::P1; // typecycling has instant timing
    let island = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let lorien = put_object(&mut state, PlayerId::P0, "Lorien Revealed", Zone::Hand);

    let before = state.clone();
    engine::step(&mut state, Action::ActivateAbility(lorien, 0)).unwrap();
    assert_eq!(
        state.players, before.players,
        "the action only stages activation"
    );
    assert_eq!(state.stack, before.stack);
    assert!(state.engine.pending_activation.is_some());

    let snapshot = state.clone();
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert!(state.objects.get(island).tapped);
    assert_eq!(state.objects.get(lorien).zone, Zone::Graveyard);
    assert!(state.players[0].graveyard.contains(&lorien));
    let item = state.stack.last().expect("cycling ability uses stack");
    assert_eq!(item.kind, StackItemKind::ActivatedAbility);
    assert_eq!(item.source, lorien);
    assert_eq!(item.discarded, vec![lorien]);
    assert_eq!(item.v4.paid_cost_refs.len(), 1);
    let paid = item.v4.paid_cost_refs[0];
    assert_eq!(paid.object, lorien);
    assert_eq!(paid.card_def, card_id("Lorien Revealed"));
    assert_eq!(paid.zone, Zone::Graveyard);
    assert_eq!(
        paid.zone_change_count,
        state.objects.get(lorien).zone_change_count
    );
    assert_eq!(paid.visible_to_mask, 0b11);
    assert!(state
        .objects
        .get(lorien)
        .v4
        .ability_uses_this_turn
        .is_empty());
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let observed_paid = &observation.projection.stack.last().unwrap().paid_cost_refs;
    assert_eq!(observed_paid.len(), 1);
    assert_eq!(observed_paid[0].arena_id, lorien.0);
    assert_eq!(observed_paid[0].zone, Zone::Graveyard);
    assert_eq!(
        observed_paid[0].zone_change_count,
        state.objects.get(lorien).zone_change_count
    );
    assert_eq!(
        &state.engine.event_history[state.engine.event_history.len() - 3..],
        &[
            CommittedEvent::Tap { object: island },
            CommittedEvent::ManaAdded {
                player: PlayerId::P0,
                colors: vec![mtg_kernel::mana::ManaColor::U],
            },
            CommittedEvent::ZoneChange {
                object: lorien,
                from: Zone::Hand,
                to: Zone::Graveyard,
                controller_before: PlayerId::P0,
            },
        ]
    );
    assert!(!state
        .engine
        .event_history
        .iter()
        .any(|event| matches!(event, CommittedEvent::SpellCast { spell, .. } if *spell == lorien)));

    let mut restored = snapshot;
    let restored_decision = engine::advance_until_decision(&mut restored);
    assert_eq!(restored_decision, decision);
    assert_eq!(restored, state);
    assert_eq!(
        restored.diagnostic_state_hash(),
        state.diagnostic_state_hash()
    );
}

#[test]
fn generated_typecycling_resolves_select_and_finish_end_to_end() {
    let mut state = ready_main(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        0x4c4f_5249_454e_0017,
    );
    let island = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let lorien = put_object(&mut state, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    let original_library = state.players[0].library.clone();

    let priority = engine::advance_until_decision(&mut state);
    assert!(matches!(
        priority,
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    let priority_actions = action_candidates(&state, &priority);
    assert_eq!(priority_actions.len(), 3);
    assert!(matches!(
        &priority_actions[0].record.semantic,
        ActionSemanticV1::ActivateManaAbility { source, .. }
            if source.arena_id == island.0
    ));
    assert!(matches!(
        &priority_actions[1].record.semantic,
        ActionSemanticV1::ActivateAbility {
            source,
            ability_index: 0,
            ..
        } if source.arena_id == lorien.0
    ));
    assert_eq!(
        priority_actions[1].record.stable_id,
        "legal-action-v4:614d110d3b5c5e52"
    );
    assert!(matches!(
        priority_actions[2].record.semantic,
        ActionSemanticV1::Pass { .. }
    ));

    engine::step(&mut state, Action::ActivateAbility(lorien, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert!(state.objects.get(island).tapped);
    assert_eq!(state.objects.get(lorien).zone, Zone::Graveyard);
    assert!(matches!(
        state.stack.last(),
        Some(StackItem {
            kind: StackItemKind::ActivatedAbility,
            source,
            ..
        }) if *source == lorien
    ));

    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    let search = engine::advance_until_decision(&mut state);
    assert!(matches!(search, Decision::ChooseEffectTargets { .. }));
    let search_actions = action_candidates(&state, &search);
    assert_eq!(
        search_actions
            .iter()
            .map(|candidate| candidate.record.stable_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "legal-action-v4:18d5d949b8b69949",
            "legal-action-v4:9660dc271fc089ff",
            "legal-action-v4:f32a4f46691a47f7",
        ]
    );
    assert!(matches!(
        search_actions[0].record.semantic,
        ActionSemanticV1::ChooseEffectTarget { .. }
    ));
    assert!(matches!(
        search_actions[1].record.semantic,
        ActionSemanticV1::ChooseEffectTarget { .. }
    ));
    assert!(matches!(
        search_actions[2].record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    ));

    let mut finish = state.clone();
    let selected = original_library[2];
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    let mut remaining = original_library.clone();
    remaining.retain(|&object| object != selected);
    let expected_selected_library =
        reference_shuffle(remaining, *state.legacy_rng().expect("legacy state"));
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.players[0].hand.contains(&selected));
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|entry| entry.object == selected));
    assert_eq!(state.players[0].library, expected_selected_library);
    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
    assert_eq!(state.objects.get(lorien).zone, Zone::Graveyard);
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_effect.is_none());

    engine::step(&mut finish, Action::FinishEffectSelection).unwrap();
    let expected_finished_library = reference_shuffle(
        original_library,
        *finish.legacy_rng().expect("legacy state"),
    );
    assert!(matches!(
        engine::advance_until_decision(&mut finish),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(finish.players[0].hand.is_empty());
    assert_eq!(finish.players[0].library, expected_finished_library);
    assert!(!finish.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::ZoneChange {
            from: Zone::Library,
            to: Zone::Hand,
            ..
        }
    )));
    assert!(finish
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(finish
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
    assert_eq!(finish.objects.get(lorien).zone, Zone::Graveyard);
    assert!(finish.stack.is_empty());
    assert!(finish.engine.pending_effect.is_none());
}

#[test]
fn typecycling_is_hand_only_instant_speed_and_casting_stays_sorcery_speed() {
    let mut no_mana = ready_main(&["Island"], 0x4c4f_5249_454e_0011);
    let lorien = put_object(&mut no_mana, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    assert!(engine::step(&mut no_mana, Action::ActivateAbility(lorien, 0)).is_err());

    let mut instant = ready_main(&["Island"], 0x4c4f_5249_454e_0012);
    instant.active_player = PlayerId::P1;
    instant.step = Step::DeclareBlockers;
    let island = put_object(&mut instant, PlayerId::P0, "Island", Zone::Battlefield);
    let lorien = put_object(&mut instant, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    instant.stack.push(StackItem {
        kind: StackItemKind::TriggeredAbility,
        source: island,
        controller: PlayerId::P1,
        targets: Vec::new(),
        is_copy: false,
        inline_effect: Some(EffectOp::Sequence(vec![])),
        discarded: Vec::new(),
        is_flashback: false,
        mode_chosen: 0,
        madness_offer: false,
        kicked: false,
        v4: StackStateV4::default(),
    });
    assert!(engine::step(&mut instant, Action::CastSpell(lorien)).is_err());
    engine::step(&mut instant, Action::ActivateAbility(lorien, 0)).unwrap();
    assert!(instant.engine.pending_activation.is_some());

    let mut wrong_zone = ready_main(&["Island"], 0x4c4f_5249_454e_0013);
    put_object(&mut wrong_zone, PlayerId::P0, "Island", Zone::Battlefield);
    let grave_lorien = put_object(
        &mut wrong_zone,
        PlayerId::P0,
        "Lorien Revealed",
        Zone::Graveyard,
    );
    assert!(engine::step(&mut wrong_zone, Action::ActivateAbility(grave_lorien, 0)).is_err());
}

#[test]
fn search_candidates_are_physical_sorted_private_stable_and_finish_last() {
    let names = ["Island", "Mountain", "Idyllic Beachfront", "Island"];
    let (mut state, source, library) = ready_inline_search(&names, 0x4c4f_5249_454e_0002);
    let decision = engine::advance_until_decision(&mut state);
    let expected = vec![library[2], library[0], library[3]];
    assert!(matches!(
        &decision,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            source: actual,
            selected_count: 0,
            min_targets: 0,
            max_targets: 1,
            legal_targets,
            can_finish: true,
        } if *actual == source
            && legal_targets == &expected.iter().copied().map(Target::Object).collect::<Vec<_>>()
    ));

    let actions = action_candidates(&state, &decision);
    assert_eq!(actions.len(), 4);
    for (candidate, object) in actions[..3].iter().zip(&expected) {
        assert!(matches!(
            &candidate.record.semantic,
            ActionSemanticV1::ChooseEffectTarget { target: mtg_kernel::rl::TargetRefV1::Object { object: stable }, .. }
                if stable.arena_id == object.0
        ));
    }
    assert!(matches!(
        &actions.last().unwrap().record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    ));
    let clone_actions = action_candidates(&state.clone(), &decision);
    assert_eq!(
        actions
            .iter()
            .map(|a| &a.record.stable_id)
            .collect::<Vec<_>>(),
        clone_actions
            .iter()
            .map(|a| &a.record.stable_id)
            .collect::<Vec<_>>()
    );

    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    let owner = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let opponent = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert!(matches!(
        owner.projection.engine_context.pending_effect.as_ref().unwrap().choice.as_ref().unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            player: PlayerSeatV1::P0,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ordered: false,
            purpose: TargetSelectionPurposeV4::SearchResult,
            ..
        } if legal_targets.len() == 3
    ));
    assert!(matches!(
        opponent.projection.engine_context.pending_effect.as_ref().unwrap().choice.as_ref().unwrap(),
        PendingEffectChoiceSemanticV4::Targets {
            legal_targets,
            selected_targets,
            min_targets: 0,
            max_targets: 0,
            can_finish: true,
            purpose: TargetSelectionPurposeV4::SearchResult,
            ..
        } if legal_targets.is_empty() && selected_targets.is_empty()
    ));
    let opponent_json = serde_json::to_string(&opponent).unwrap();
    assert!(!opponent_json.contains("Idyllic Beachfront"));
    assert!(!opponent_json.contains("Island"));
}

#[test]
fn search_uses_effective_subtypes_and_fails_closed_when_effective_types_are_unavailable() {
    let (mut dynamic, _, library) = ready_inline_search(
        &["Island", "Mountain", "Fiery Temper"],
        0x4c4f_5249_454e_0014,
    );
    dynamic
        .objects
        .get_mut(library[0])
        .v4
        .effective_subtype_ids
        .clear();
    let gained = &mut dynamic.objects.get_mut(library[1]).v4.effective_subtype_ids;
    gained.push(Subtype::Island.stable_id());
    gained.sort_unstable();
    let decision = engine::advance_until_decision(&mut dynamic);
    assert!(matches!(
        decision,
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets == vec![Target::Object(library[1])]
    ));

    let (mut unavailable, source, library) = ready_inline_search(
        &["Island", "Mountain", "Fiery Temper"],
        0x4c4f_5249_454e_0015,
    );
    unavailable.objects.get_mut(library[0]).v4.face_index = 1;
    let original_library = unavailable.players[0].library.clone();
    let rng = *unavailable.legacy_rng().expect("legacy state");
    assert!(matches!(
        engine::advance_until_decision(&mut unavailable),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: actual,
        } if actual == source
    ));
    assert_eq!(unavailable.players[0].library, original_library);
    assert_eq!(*unavailable.legacy_rng().expect("legacy state"), rng);
}

#[test]
fn choose_moves_then_reveals_then_deterministically_shuffles_remaining_library() {
    let (mut state, _source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        0x4c4f_5249_454e_0003,
    );
    let decision = engine::advance_until_decision(&mut state);
    let selected = library[2];
    assert!(matches!(decision, Decision::ChooseEffectTargets { .. }));
    let before_answer = state.clone();
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    assert_eq!(state.players[0].library, before_answer.players[0].library);
    assert!(!state.players[0].hand.contains(&selected));
    assert!(state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .is_none());

    let post_answer = state.clone();
    let mut unshuffled = library.clone();
    unshuffled.retain(|&object| object != selected);
    let expected = reference_shuffle(unshuffled, *post_answer.legacy_rng().expect("legacy state"));
    let resolved_decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        resolved_decision,
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.players[0].library, expected);
    assert!(state.players[0].hand.contains(&selected));
    assert_eq!(state.objects.get(selected).zone, Zone::Hand);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .iter()
        .any(|entry| entry.object == selected));
    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
    assert!(state.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::ZoneChange {
            object,
            from: Zone::Library,
            to: Zone::Hand,
            ..
        } if *object == selected
    )));

    let mut restored = post_answer;
    let restored_decision = engine::advance_until_decision(&mut restored);
    assert_eq!(restored_decision, resolved_decision);
    assert_eq!(restored, state);
}

#[test]
fn finish_with_matches_and_zero_match_or_empty_library_all_still_shuffle() {
    let (mut finish, _, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        0x4c4f_5249_454e_0004,
    );
    engine::advance_until_decision(&mut finish);
    let expected = reference_shuffle(library.clone(), *finish.legacy_rng().expect("legacy state"));
    engine::step(&mut finish, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut finish),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(finish.players[0].library, expected);
    assert!(finish.players[0].hand.is_empty());

    let (mut no_match, _, library) = ready_inline_search(
        &["Mountain", "Fiery Temper", "Snow-Covered Forest"],
        0x4c4f_5249_454e_0005,
    );
    let expected = reference_shuffle(library, *no_match.legacy_rng().expect("legacy state"));
    let no_match_decision = engine::advance_until_decision(&mut no_match);
    assert!(matches!(
        &no_match_decision,
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        } if legal_targets.is_empty()
    ));
    let no_match_actions = action_candidates(&no_match, &no_match_decision);
    assert_eq!(no_match_actions.len(), 1);
    assert_eq!(
        no_match_actions[0].record.stable_id,
        "legal-action-v4:3ae395f28a70411e"
    );
    assert!(matches!(
        no_match_actions[0].record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    ));
    let no_match_opponent =
        observe_v2(&no_match, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();

    let (mut has_match, _, _) = ready_inline_search(
        &["Island", "Fiery Temper", "Snow-Covered Forest"],
        0x4c4f_5249_454e_0016,
    );
    let has_match_decision = engine::advance_until_decision(&mut has_match);
    assert!(matches!(
        &has_match_decision,
        Decision::ChooseEffectTargets {
            legal_targets,
            ..
        } if legal_targets.len() == 1
    ));
    let has_match_actions = action_candidates(&has_match, &has_match_decision);
    assert_eq!(has_match_actions.len(), 2);
    assert_eq!(
        has_match_actions.last().unwrap().record.stable_id,
        no_match_actions[0].record.stable_id,
        "Finish keeps one semantic id regardless of hidden match count"
    );
    let has_match_opponent =
        observe_v2(&has_match, &HarnessSurfaceV2::new(), PlayerId::P1, 0).unwrap();
    assert_eq!(
        no_match_opponent
            .projection
            .engine_context
            .pending_effect
            .as_ref(),
        has_match_opponent
            .projection
            .engine_context
            .pending_effect
            .as_ref(),
        "a nonchooser must not learn whether the private search has a match"
    );
    assert_eq!(
        no_match_opponent.visible_projection_hash, has_match_opponent.visible_projection_hash,
        "hidden match existence must not perturb the nonchooser projection hash"
    );

    engine::step(&mut no_match, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut no_match),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(no_match.players[0].library, expected);
    assert!(no_match.players[0].hand.is_empty());
    assert!(!no_match.players[0].has_lost);

    let (mut empty, _, _) = ready_inline_search(&[], 0x4c4f_5249_454e_0006);
    let empty_decision = engine::advance_until_decision(&mut empty);
    assert!(matches!(
        &empty_decision,
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty()
    ));
    let empty_actions = action_candidates(&empty, &empty_decision);
    assert_eq!(empty_actions.len(), 1);
    assert!(matches!(
        empty_actions[0].record.semantic,
        ActionSemanticV1::FinishEffectSelection { .. }
    ));
    engine::step(&mut empty, Action::FinishEffectSelection).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut empty),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(empty.players[0].library.is_empty());
    assert!(!empty.players[0].has_lost, "searching is not drawing");
}

fn assert_invalid_continuation_without_gameplay_mutation(mut state: GameState, source: ObjectId) {
    let library = state.players[0].library.clone();
    let hand = state.players[0].hand.clone();
    let history = state.engine.event_history.clone();
    let rng = *state.legacy_rng().expect("legacy state");
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: actual,
        } if actual == source
    ));
    assert_eq!(state.players[0].library, library);
    assert_eq!(state.players[0].hand, hand);
    assert_eq!(state.engine.event_history, history);
    assert_eq!(*state.legacy_rng().expect("legacy state"), rng);
}

#[test]
fn stale_search_prompt_and_completion_frame_fail_closed_before_any_mutation() {
    let (mut base, source, _) = ready_inline_search(
        &["Island", "Idyllic Beachfront", "Mountain"],
        0x4c4f_5249_454e_0007,
    );
    let decision = engine::advance_until_decision(&mut base);

    let mut chooser = base.clone();
    if let Some(mtg_kernel::effect::PendingEffectChoice::SelectTargets { player, .. }) = chooser
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    {
        *player = PlayerId::P1;
    }
    assert_invalid_continuation_without_gameplay_mutation(chooser, source);

    let mut partition = base.clone();
    if let Some(mtg_kernel::effect::PendingEffectChoice::SelectTargets { legal, .. }) = partition
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    {
        legal.pop();
    }
    assert_invalid_continuation_without_gameplay_mutation(partition, source);

    let mut filter = base.clone();
    if let Some(mtg_kernel::effect::PendingEffectChoice::SelectTargets { purpose, .. }) = filter
        .engine
        .pending_effect
        .as_mut()
        .and_then(|pending| pending.choice.as_mut())
    {
        let EffectTargetSelectionPurpose::SearchLibraryToHand { filter, .. } = purpose else {
            panic!("search purpose")
        };
        *filter = LibraryCardFilter::LandWithSubtype(Subtype::Forest);
    }
    assert_invalid_continuation_without_gameplay_mutation(filter, source);

    let selected = match decision {
        Decision::ChooseEffectTargets { legal_targets, .. } => legal_targets[0],
        _ => panic!("search decision"),
    };
    let mut frame = base;
    engine::step(&mut frame, Action::ChooseEffectTarget(selected)).unwrap();
    let mut frame_filter = frame.clone();
    let EffectFrame::SearchLibraryToHand { filter, .. } = frame_filter
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .last_mut()
        .unwrap()
    else {
        panic!("search completion frame")
    };
    *filter = LibraryCardFilter::LandWithSubtype(Subtype::Forest);
    assert_invalid_continuation_without_gameplay_mutation(frame_filter, source);

    let EffectFrame::SearchLibraryToHand {
        original_library, ..
    } = frame
        .engine
        .pending_effect
        .as_mut()
        .unwrap()
        .frames
        .last_mut()
        .unwrap()
    else {
        panic!("search completion frame")
    };
    original_library.swap(0, 1);
    assert_invalid_continuation_without_gameplay_mutation(frame, source);
}

#[test]
fn stale_pending_activation_halts_before_mana_discard_or_stack_mutation() {
    let mut state = ready_main(&["Island"], 0x4c4f_5249_454e_0008);
    let island = put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    let lorien = put_object(&mut state, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    engine::step(&mut state, Action::ActivateAbility(lorien, 0)).unwrap();
    state
        .engine
        .pending_activation
        .as_mut()
        .unwrap()
        .source_zone_change_count += 1;
    let history = state.engine.event_history.clone();
    let decision = engine::advance_until_decision(&mut state);
    assert!(
        matches!(
            decision,
            Decision::Halted {
                mechanic: UnsupportedMechanic::InvalidEffectContinuation,
                source,
            } if source == lorien
        ),
        "{decision:?}"
    );
    assert!(!state.objects.get(island).tapped);
    assert_eq!(state.objects.get(lorien).zone, Zone::Hand);
    assert!(state.stack.is_empty());
    assert_eq!(state.engine.event_history, history);
}

#[test]
fn interactive_activation_cannot_self_authenticate_a_restored_payment() {
    let mut state = ready_main(&["Island"], 0x4c4f_5249_454e_0018);
    let source = put_object(&mut state, PlayerId::P0, "Masked Meower", Zone::Battlefield);
    let decoy = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Fiery Temper", Zone::Hand);
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    state
        .engine
        .pending_activation
        .as_mut()
        .unwrap()
        .cost_discard_paid = Some(vec![decoy]);

    let history = state.engine.event_history.clone();
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: halted_source,
        } if halted_source == source
    ));
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.players[0].hand.contains(&decoy));
    assert!(state.stack.is_empty());
    assert_eq!(state.engine.event_history, history);
}

#[test]
fn activation_discard_cross_slot_mismatch_halts_before_any_payment_mutation() {
    let mut state = ready_main(&["Island"], 0x4c4f_5249_454e_0019);
    let source = put_object(&mut state, PlayerId::P0, "Masked Meower", Zone::Battlefield);
    let first = put_object(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    let second = put_object(&mut state, PlayerId::P0, "Fiery Temper", Zone::Hand);
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Discard {
            player: PlayerId::P0,
            count: 1,
            ..
        }
    ));
    state.engine.pending_discard.as_mut().unwrap().player = PlayerId::P1;

    let history = state.engine.event_history.clone();
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        decision,
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: halted_source,
        } if halted_source == source
    ));
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert_eq!(state.players[0].hand, vec![first, second]);
    assert!(state.stack.is_empty());
    assert_eq!(state.engine.event_history, history);
}

#[test]
fn diagnostic_hash_covers_pending_activation_and_discard_binding() {
    let mut state = ready_main(&["Island"], 0x4c4f_5249_454e_001a);
    let source = put_object(&mut state, PlayerId::P0, "Masked Meower", Zone::Battlefield);
    put_object(&mut state, PlayerId::P0, "Mountain", Zone::Hand);
    put_object(&mut state, PlayerId::P0, "Fiery Temper", Zone::Hand);
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Discard {
            player: PlayerId::P0,
            count: 1,
            ..
        }
    ));

    let restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(restored, state);
    assert_eq!(
        restored.diagnostic_state_hash(),
        state.diagnostic_state_hash()
    );

    let mut activation_tamper = state.clone();
    activation_tamper
        .engine
        .pending_activation
        .as_mut()
        .unwrap()
        .source_zone_change_count += 1;
    assert_ne!(
        activation_tamper.diagnostic_state_hash(),
        state.diagnostic_state_hash()
    );

    let mut discard_tamper = state.clone();
    let mtg_kernel::engine::DiscardResume::FinishActivation { controller, .. } =
        &mut discard_tamper
            .engine
            .pending_discard
            .as_mut()
            .unwrap()
            .resume
    else {
        panic!("activation discard must carry a structured resume binding")
    };
    *controller = PlayerId::P1;
    assert_ne!(
        discard_tamper.diagnostic_state_hash(),
        state.diagnostic_state_hash()
    );
}

#[test]
fn countered_cycling_item_keeps_paid_costs_and_registered_spell_draws_three() {
    let mut cycling = ready_main(&["Island", "Mountain"], 0x4c4f_5249_454e_0009);
    let library = cycling.players[0].library.clone();
    let island = put_object(&mut cycling, PlayerId::P0, "Island", Zone::Battlefield);
    let lorien = put_object(&mut cycling, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    engine::step(&mut cycling, Action::ActivateAbility(lorien, 0)).unwrap();
    engine::advance_until_decision(&mut cycling);
    let rng = *cycling.legacy_rng().expect("legacy state");
    let removed = cycling
        .stack
        .pop()
        .expect("ability can be countered/removed from stack");
    assert_eq!(removed.kind, StackItemKind::ActivatedAbility);
    assert!(cycling.objects.get(island).tapped);
    assert_eq!(cycling.objects.get(lorien).zone, Zone::Graveyard);
    assert_eq!(cycling.players[0].library, library);
    assert_eq!(
        *cycling.legacy_rng().expect("legacy state"),
        rng,
        "a countered ability never searches or shuffles"
    );

    let library_defs = ["Mountain", "Fiery Temper", "Island", "Brainstorm"].map(card_id);
    let mut spell = GameState::new_from_libraries(
        &library_defs,
        &[card_id("Snow-Covered Forest")],
        card_name,
        0x4c4f_5249_454e_0010,
    );
    spell.step = Step::Main1;
    spell.active_player = PlayerId::P0;
    spell.priority_player = PlayerId::P0;
    for _ in 0..5 {
        put_object(&mut spell, PlayerId::P0, "Island", Zone::Battlefield);
    }
    let lorien_spell = put_object(&mut spell, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    engine::step(&mut spell, Action::CastSpell(lorien_spell)).unwrap();
    let hand_before = spell.players[0].hand.len();
    for _ in 0..12 {
        let decision = engine::advance_until_decision(&mut spell);
        if spell.objects.get(lorien_spell).zone == Zone::Graveyard {
            break;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(&mut spell, Action::Pass).unwrap(),
            other => panic!("unexpected Lorien spell decision: {other:?}"),
        }
    }
    assert_eq!(spell.objects.get(lorien_spell).zone, Zone::Graveyard);
    assert_eq!(spell.players[0].hand.len(), hand_before + 3);
    assert_eq!(
        spell
            .engine
            .event_history
            .iter()
            .filter(|event| matches!(
                event,
                CommittedEvent::Draw {
                    player: PlayerId::P0,
                    ..
                }
            ))
            .count(),
        3
    );
}

#[test]
fn a_normally_countered_lorien_spell_draws_nothing() {
    let p0_library = ["Mountain", "Fiery Temper", "Brainstorm"].map(card_id);
    let mut state = GameState::new_from_libraries(
        &p0_library,
        &[card_id("Snow-Covered Forest")],
        card_name,
        0x4c4f_5249_454e_0014,
    );
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    for _ in 0..5 {
        put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
    }
    for _ in 0..2 {
        put_object(&mut state, PlayerId::P1, "Island", Zone::Battlefield);
    }
    let lorien = put_object(&mut state, PlayerId::P0, "Lorien Revealed", Zone::Hand);
    let counterspell = put_object(&mut state, PlayerId::P1, "Counterspell", Zone::Hand);
    let library_before = state.players[0].library.clone();

    engine::step(&mut state, Action::CastSpell(lorien)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseTargets {
            player: PlayerId::P1,
            legal_targets,
            ..
        } if legal_targets.contains(&Target::Object(lorien))
    ));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(lorien))).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut state, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));

    assert_eq!(state.objects.get(lorien).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    assert_eq!(state.players[0].library, library_before);
    assert!(!state.engine.event_history.iter().any(|event| matches!(
        event,
        CommittedEvent::Draw {
            player: PlayerId::P0,
            ..
        }
    )));
}

/// Swaps a staged state's legacy randomness for the given environment-v2
/// fragment through the public serde surface, exactly as a restore would.
fn into_environment_v2(state: &GameState, seed: u64, fragment: &str) -> GameState {
    let json = serde_json::to_string(state).expect("staged state serializes");
    let legacy_fragment = format!("\"rng\":{{\"state\":{seed}}}");
    assert!(
        json.contains(&legacy_fragment),
        "staged state must carry the untouched legacy seed fragment"
    );
    serde_json::from_str(&json.replacen(&legacy_fragment, fragment, 1))
        .expect("converted v2 state deserializes")
}

#[test]
fn v2_search_commits_the_derived_substream_shuffle_at_the_frame() {
    use mtg_kernel::environment_randomization_v2::{
        derive_environment_randomization_seed_v2, PhysicalOwnerV2, ShufflePurposeV2,
    };
    let seed = 0x4c4f_5249_454e_1001;
    let (staged, _source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        seed,
    );
    let mut state = into_environment_v2(
        &staged,
        seed,
        "\"environment_randomization_v2\":{\"pair_environment_seed\":424242,\"next_live_shuffle_ordinal\":[0,0]}",
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    let selected = library[2];
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    assert_eq!(
        state
            .environment_randomization_v2()
            .expect("v2 state")
            .next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
        0,
        "the action-time preflight token is discarded without consuming an ordinal"
    );
    let mut remaining = library.clone();
    remaining.retain(|&object| object != selected);
    let derived = derive_environment_randomization_seed_v2(
        424242,
        PhysicalOwnerV2::P0,
        ShufflePurposeV2::InGameLibraryShuffle,
        0,
    )
    .expect("module derivation");
    let expected = reference_shuffle(remaining, mtg_kernel::state::SplitMix64::seed(derived));
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.players[0].hand.contains(&selected));
    assert_eq!(
        state.players[0].library, expected,
        "the v2 frame must shuffle with exactly the derived substream permutation"
    );
    let v2 = state.environment_randomization_v2().expect("v2 state");
    assert_eq!(
        v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
        1,
        "the frame commit consumes exactly one P0 ordinal"
    );
    assert_eq!(v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P1), 0);
    assert!(state
        .known_library_cards(PlayerId::P0, PlayerId::P0)
        .is_empty());
    assert!(state
        .known_library_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
}

#[test]
fn v2_search_actions_reject_exhausted_ordinals_byte_exact() {
    let seed = 0x4c4f_5249_454e_1002;
    let (staged, _source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        seed,
    );
    let mut state = into_environment_v2(
        &staged,
        seed,
        "\"environment_randomization_v2\":{\"pair_environment_seed\":424242,\"next_live_shuffle_ordinal\":[18446744073709551615,0]}",
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    let before_json = serde_json::to_string(&state).expect("serializes");
    let before_hot = state.state_hash();
    let before_diag = state.diagnostic_state_hash();
    let selected = library[2];
    assert!(
        engine::step(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(selected)),
        )
        .is_err(),
        "a target choice at an exhausted ordinal must fail"
    );
    assert_eq!(
        serde_json::to_string(&state).expect("serializes"),
        before_json,
        "the failed target choice must be byte-exact nonmutating"
    );
    assert_eq!(state.state_hash(), before_hot);
    assert_eq!(state.diagnostic_state_hash(), before_diag);
    assert!(
        engine::step(&mut state, Action::FinishEffectSelection).is_err(),
        "a zero-selection finish at an exhausted ordinal must fail"
    );
    assert_eq!(
        serde_json::to_string(&state).expect("serializes"),
        before_json,
        "the failed finish must be byte-exact nonmutating"
    );
    assert_eq!(state.state_hash(), before_hot);
    assert_eq!(state.diagnostic_state_hash(), before_diag);
}

#[test]
fn v2_finish_with_zero_matches_still_consumes_one_ordinal() {
    use mtg_kernel::environment_randomization_v2::{
        derive_environment_randomization_seed_v2, PhysicalOwnerV2, ShufflePurposeV2,
    };
    let seed = 0x4c4f_5249_454e_1003;
    let (staged, _source, library) =
        ready_inline_search(&["Mountain", "Fiery Temper", "Snow-Covered Forest"], seed);
    let mut state = into_environment_v2(
        &staged,
        seed,
        "\"environment_randomization_v2\":{\"pair_environment_seed\":515151,\"next_live_shuffle_ordinal\":[0,0]}",
    );
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        &decision,
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty()
    ));
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    assert_eq!(
        state
            .environment_randomization_v2()
            .expect("v2 state")
            .next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
        0,
        "the finish-time preflight token is discarded without consuming an ordinal"
    );
    let derived = derive_environment_randomization_seed_v2(
        515151,
        PhysicalOwnerV2::P0,
        ShufflePurposeV2::InGameLibraryShuffle,
        0,
    )
    .expect("module derivation");
    let expected = reference_shuffle(library, mtg_kernel::state::SplitMix64::seed(derived));
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(
        state.players[0].hand.is_empty(),
        "a zero-match search moves nothing"
    );
    assert_eq!(
        state.players[0].library, expected,
        "the fail-to-find shuffle still commits the derived substream"
    );
    let v2 = state.environment_randomization_v2().expect("v2 state");
    assert_eq!(
        v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
        1,
        "fail-to-find still consumes exactly one ordinal"
    );
    assert_eq!(v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P1), 0);
}

#[test]
fn v2_answered_search_frame_survives_serde_round_trip_and_replays_identically() {
    let seed = 0x4c4f_5249_454e_1004;
    let (staged, _source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        seed,
    );
    let mut state = into_environment_v2(
        &staged,
        seed,
        "\"environment_randomization_v2\":{\"pair_environment_seed\":616161,\"next_live_shuffle_ordinal\":[0,0]}",
    );
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    let selected = library[2];
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    let mut round_tripped: GameState =
        serde_json::from_str(&serde_json::to_string(&state).expect("answered frame serializes"))
            .expect("answered frame deserializes");
    assert_eq!(state, round_tripped);
    let original_decision = engine::advance_until_decision(&mut state);
    let replayed_decision = engine::advance_until_decision(&mut round_tripped);
    assert!(matches!(
        &original_decision,
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(original_decision, replayed_decision);
    assert_eq!(
        state, round_tripped,
        "the round-tripped answered Search frame must replay identically"
    );
    assert!(state.players[0].hand.contains(&selected));
}

#[test]
fn v2_answered_search_with_live_noncandidate_selected_halts_without_any_mutation() {
    use mtg_kernel::environment_randomization_v2::PhysicalOwnerV2;
    let seed = 0x4c4f_5249_454e_1005;
    let (staged, source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        seed,
    );
    let mut state = into_environment_v2(
        &staged,
        seed,
        "\"environment_randomization_v2\":{\"pair_environment_seed\":717171,\"next_live_shuffle_ordinal\":[0,0]}",
    );
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, 2);
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, 1);
    let mountain = library[1];
    // The engine's own canonical candidate computation excludes the
    // Mountain: it is a true filter nonmatch for LandWithSubtype(Island).
    let decision = engine::advance_until_decision(&mut state);
    assert!(matches!(
        &decision,
        Decision::ChooseEffectTargets { legal_targets, .. }
            if !legal_targets.is_empty()
                && !legal_targets.contains(&Target::Object(mountain))
    ));
    let selected = library[2];
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    // Tamper the answered frame only: replace its `selected` with the live
    // Mountain binding copied from that same frame's original_library. All
    // ObjectState bytes stay untouched, so the replacement is a live,
    // correctly bound library object that is simply not a candidate.
    let mountain_binding = {
        let continuation = state
            .engine
            .pending_effect
            .as_mut()
            .expect("answered continuation is pending");
        let (frame_original_library, frame_selected) = continuation
            .frames
            .iter_mut()
            .find_map(|frame| match frame {
                EffectFrame::SearchLibraryToHand {
                    original_library,
                    selected,
                    ..
                } => Some((original_library, selected)),
                _ => None,
            })
            .expect("answered SearchLibraryToHand frame is staged");
        let binding = *frame_original_library
            .iter()
            .find(|binding| binding.object == mountain)
            .expect("Mountain binding is in the frame's original_library");
        *frame_selected = Some(binding);
        binding
    };
    // Complete-object sentinels: every byte of both live GameObjects (the
    // Mountain replacement and the originally selected card), including
    // zone, face, counters, controller, damage, and generation fields, must
    // survive the halted continuation unchanged.
    let mountain_object_before = state.objects.get(mountain).clone();
    let selected_object_before = state.objects.get(selected).clone();
    // The replacement object remains in P0's library at its exact bound
    // incarnation, front face up.
    assert!(state.players[0].library.contains(&mountain));
    assert_eq!(state.objects.get(mountain).zone, Zone::Library);
    assert_eq!(
        state.objects.get(mountain).zone_change_count,
        mountain_binding.expected_zone_change_count,
        "the replacement binding matches the live incarnation"
    );
    assert_eq!(
        state.objects.get(mountain).v4.face_index,
        0,
        "the replacement object's front face is untouched"
    );
    let library_before = state.players[0].library.clone();
    let hand_before = state.players[0].hand.clone();
    let stack_before = state.stack.clone();
    let library_knowledge_before = state.library_knowledge.clone();
    let hand_knowledge_before = state.hand_knowledge.clone();
    let history_before = state.engine.event_history.clone();
    for observer in [PlayerId::P0, PlayerId::P1] {
        assert!(
            !library_knowledge_before[observer.index()][PlayerId::P0.index()].is_empty(),
            "observer {observer:?} knowledge sentinel must be nonempty"
        );
    }
    assert!(!stack_before.is_empty(), "stack sentinel must be nonempty");
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: halted,
        } if halted == source
    ));
    assert_eq!(state.players[0].library, library_before, "no shuffle");
    assert!(
        state.players[0].library.contains(&mountain),
        "no move: the noncandidate replacement stays in the library"
    );
    assert!(
        state.players[0].library.contains(&selected),
        "no move: the originally answered card stays in the library too"
    );
    assert_eq!(state.players[0].hand, hand_before, "no move to hand");
    assert_eq!(
        *state.objects.get(mountain),
        mountain_object_before,
        "the halted continuation must leave every byte of the replacement \
         GameObject unchanged"
    );
    assert_eq!(
        *state.objects.get(selected),
        selected_object_before,
        "the halted continuation must leave every byte of the originally \
         selected GameObject unchanged"
    );
    assert_eq!(
        state.stack, stack_before,
        "the halt restores the public stack exactly"
    );
    assert_eq!(
        state.library_knowledge, library_knowledge_before,
        "no library knowledge change"
    );
    assert_eq!(state.hand_knowledge, hand_knowledge_before, "no reveal");
    assert_eq!(
        state.engine.event_history, history_before,
        "no committed event"
    );
    let v2 = state.environment_randomization_v2().expect("v2 state");
    assert_eq!(
        v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P0),
        0,
        "no counter consumption"
    );
    assert_eq!(v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P1), 0);
}

#[test]
fn v2_answered_search_frame_restored_at_max_preflights_before_move_or_reveal() {
    use mtg_kernel::environment_randomization_v2::PhysicalOwnerV2;
    let seed = 0x4c4f_5249_454e_1006;
    let (staged, source, library) = ready_inline_search(
        &["Island", "Mountain", "Idyllic Beachfront", "Fiery Temper"],
        seed,
    );
    const HEALTHY: &str = "\"environment_randomization_v2\":{\"pair_environment_seed\":818181,\"next_live_shuffle_ordinal\":[0,0]}";
    const EXHAUSTED: &str = "\"environment_randomization_v2\":{\"pair_environment_seed\":818181,\"next_live_shuffle_ordinal\":[18446744073709551615,0]}";
    let mut state = into_environment_v2(&staged, seed, HEALTHY);
    state.reveal_library_top(PlayerId::P0, PlayerId::P0, 2);
    state.reveal_library_top(PlayerId::P1, PlayerId::P0, 1);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    let selected = library[2];
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(selected)),
    )
    .unwrap();
    // Restore the accepted answered frame into an exhausted P0 counter, as a
    // snapshot restore would.
    let mut restored: GameState = serde_json::from_str(
        &serde_json::to_string(&state)
            .expect("answered frame serializes")
            .replacen(HEALTHY, EXHAUSTED, 1),
    )
    .expect("restored state deserializes");
    let library_before = restored.players[0].library.clone();
    let hand_before = restored.players[0].hand.clone();
    let stack_before = restored.stack.clone();
    let library_knowledge_before = restored.library_knowledge.clone();
    let hand_knowledge_before = restored.hand_knowledge.clone();
    let history_before = restored.engine.event_history.clone();
    for observer in [PlayerId::P0, PlayerId::P1] {
        assert!(
            !library_knowledge_before[observer.index()][PlayerId::P0.index()].is_empty(),
            "observer {observer:?} knowledge sentinel must be nonempty"
        );
    }
    assert!(!stack_before.is_empty(), "stack sentinel must be nonempty");
    assert!(matches!(
        engine::advance_until_decision(&mut restored),
        Decision::Halted {
            mechanic: UnsupportedMechanic::InvalidEffectContinuation,
            source: halted,
        } if halted == source
    ));
    assert!(
        restored.players[0].library.contains(&selected),
        "the exhausted preflight must fire before the move: the selected \
         card never left the library"
    );
    assert_eq!(restored.players[0].library, library_before, "no shuffle");
    assert_eq!(restored.players[0].hand, hand_before, "no move to hand");
    assert_eq!(
        restored.stack, stack_before,
        "the halt restores the public stack exactly"
    );
    assert_eq!(
        restored.library_knowledge, library_knowledge_before,
        "no library knowledge change"
    );
    assert_eq!(
        restored.hand_knowledge, hand_knowledge_before,
        "the exhausted preflight must fire before any reveal"
    );
    assert_eq!(
        restored.engine.event_history, history_before,
        "no committed event"
    );
    let v2 = restored.environment_randomization_v2().expect("v2 state");
    assert_eq!(v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P0), u64::MAX);
    assert_eq!(v2.next_live_shuffle_ordinal(PhysicalOwnerV2::P1), 0);
}
