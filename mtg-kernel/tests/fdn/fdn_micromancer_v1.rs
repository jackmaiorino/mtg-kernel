//! Registered candidate; full gameplay/profile admission remains pending.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardType, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn ready(active: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1109,
        active,
    );
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
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
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Library => state.players[player.index()].library.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let choice = engine::advance_until_decision(state);
    assert!(!matches!(choice, Decision::Halted { .. }), "{choice:?}");
    choice
}

fn priority(state: &mut GameState, player: PlayerId) {
    let Decision::CastSpellOrPass {
        player: chooser, ..
    } = next(state)
    else {
        panic!("priority decision absent");
    };
    if chooser != player {
        engine::step(state, Action::Pass).unwrap();
        assert!(
            matches!(next(state), Decision::CastSpellOrPass { player: chooser, .. } if chooser == player)
        );
    }
}

fn cast(state: &mut GameState, spell: ObjectId, target: Option<Target>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for _ in 0..16 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("fixture target");
                assert!(legal_targets.contains(&target));
                engine::step(state, Action::ChooseTarget(target)).unwrap();
            }
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.engine.pending_cast.is_none() => return,
            choice => panic!("unexpected cast decision {choice:?}"),
        }
    }
    panic!("cast did not finish");
}

fn settle(state: &mut GameState) -> Decision {
    for _ in 0..64 {
        let choice = next(state);
        match choice {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return choice
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { ref pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap();
            }
            _ => return choice,
        }
    }
    panic!("resolution did not settle");
}

fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

fn same(state: &GameState, replay: &GameState) {
    assert_eq!(
        serde_json::to_value(state).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
}

fn stage(player: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready(player);
    let source = put(&mut state, player, "Micromancer", Zone::Hand);
    state.players[player.index()].mana_pool[1] = 1;
    state.players[player.index()].mana_pool[5] = 3;
    priority(&mut state, player);
    cast(&mut state, source, None);
    assert!(
        matches!(settle(&mut state), Decision::ChooseEffectOption { player: actor, option_count: 2, .. } if actor == player)
    );
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    (state, source)
}

fn search(state: &mut GameState, player: PlayerId) -> Vec<Target> {
    engine::step(state, Action::ChooseEffectOption(0)).unwrap();
    match settle(state) {
        Decision::ChooseEffectTargets {
            player: actor,
            legal_targets,
            min_targets: 0,
            max_targets: 1,
            can_finish: true,
            ..
        } if actor == player => legal_targets,
        decision => panic!("unexpected search {decision:?}"),
    }
}

#[test]
fn printed_metadata_and_actual_filtered_revealed_search_restore() {
    let def = &CARD_DEFS[card_id_by_name("Micromancer").unwrap() as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Human, Subtype::Wizard]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(3), Some(3), 4)
    );
    assert_eq!(def.colors, &[ManaColor::U]);
    assert_eq!(def.cost.generic, 3);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::U)]);
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = stage(player);
        let instant = put(&mut state, player, "Burst Lightning", Zone::Library);
        let sorcery = put(&mut state, player, "Duress", Zone::Library);
        let creature = put(&mut state, player, "Llanowar Elves", Zone::Library);
        let other_value = put(&mut state, player, "Think Twice", Zone::Library);
        let opposing = put(
            &mut state,
            player.opponent(),
            "Burst Lightning",
            Zone::Library,
        );
        let legal = search(&mut state, player);
        assert_eq!(legal.len(), 2);
        assert!(
            legal.contains(&Target::Object(instant)) && legal.contains(&Target::Object(sorcery))
        );
        for illegal in [creature, other_value, opposing] {
            assert!(!legal.contains(&Target::Object(illegal)));
        }
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseEffectTarget(Target::Object(sorcery))).unwrap();
            if matches!(next(current), Decision::ChooseEffectTargets { .. }) {
                engine::step(current, Action::FinishEffectSelection).unwrap();
            }
            assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
            assert_eq!(current.objects.get(sorcery).zone, Zone::Hand);
            assert!(current.players[player.index()].hand.contains(&sorcery));
            assert!(current
                .known_hand_cards(player.opponent(), player)
                .iter()
                .any(|entry| entry.object == sorcery
                    && entry.zone_change_count == current.objects.get(sorcery).zone_change_count));
            assert!(!current.players[player.index()].library.contains(&sorcery));
        }
        same(&state, &replay);
    }
}

#[test]
fn optional_decline_preserves_library_and_search_allows_failure_to_find() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = stage(player);
        let before = state.players[player.index()].library.clone();
        let rng = state.legacy_rng().copied();
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::ChooseEffectOption(1)).unwrap();
            assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
            assert_eq!(current.players[player.index()].library, before);
            assert_eq!(current.legacy_rng().copied(), rng);
        }
        same(&state, &replay);
        let (mut state, _) = stage(player);
        assert!(search(&mut state, player).is_empty());
        let mut replay = restored(&state);
        for current in [&mut state, &mut replay] {
            engine::step(current, Action::FinishEffectSelection).unwrap();
            assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
            assert_eq!(current.players[player.index()].library.len(), before.len());
        }
        same(&state, &replay);
        let (mut state, _) = stage(player);
        put(&mut state, player, "Burst Lightning", Zone::Library);
        assert_eq!(search(&mut state, player).len(), 1);
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        assert!(matches!(
            settle(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
}

#[test]
fn private_match_presence_does_not_change_nonacting_public_projection() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut empty, _) = stage(player);
        let mut found = restored(&empty);
        let object = found.players[player.index()].library[0];
        let def = card_id_by_name("Burst Lightning").unwrap();
        let card = found.objects.get_mut(object);
        card.card_def = def;
        card.name = "Burst Lightning".into();
        card.v4 = ObjectStateV4::from_card_def(def);
        assert!(search(&mut empty, player).is_empty());
        assert_eq!(search(&mut found, player).len(), 1);
        assert_eq!(
            serde_json::to_value(
                mtg_kernel::rl::observe_policy_v6(
                    &empty,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    player.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap()
            )
            .unwrap(),
            serde_json::to_value(
                mtg_kernel::rl::observe_policy_v6(
                    &found,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    player.opponent(),
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap()
            )
            .unwrap()
        );
    }
}

#[test]
fn captured_controller_search_survives_source_departure_or_control_change() {
    for player in [PlayerId::P0, PlayerId::P1] {
        for leave in [false, true] {
            let (mut state, source) = stage(player);
            let card = put(&mut state, player, "Burst Lightning", Zone::Library);
            if leave {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
            } else {
                state.objects.get_mut(source).controller = player.opponent();
                state.players[player.index()]
                    .battlefield
                    .retain(|id| *id != source);
                state.players[player.opponent().index()]
                    .battlefield
                    .push(source);
            }
            assert_eq!(search(&mut state, player), vec![Target::Object(card)]);
            let mut replay = restored(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
                if matches!(next(current), Decision::ChooseEffectTargets { .. }) {
                    engine::step(current, Action::FinishEffectSelection).unwrap();
                }
                assert!(matches!(settle(current), Decision::CastSpellOrPass { .. }));
                assert!(current.players[player.index()].hand.contains(&card));
                assert!(!current.players[player.opponent().index()]
                    .hand
                    .contains(&card));
            }
            same(&state, &replay);
        }
    }
}

#[test]
fn forged_filter_fingerprint_and_candidate_bindings_refuse_without_state_mutation() {
    use mtg_kernel::effect::{
        EffectTargetSelectionPurpose, LibraryCardFilter, PendingEffectChoice,
    };
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = stage(player);
        put(&mut state, player, "Burst Lightning", Zone::Library);
        let opposing = put(
            &mut state,
            player.opponent(),
            "Burst Lightning",
            Zone::Library,
        );
        assert_eq!(search(&mut state, player).len(), 1);
        for change in 0..3 {
            let mut forged = restored(&state);
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            let Some(PendingEffectChoice::SelectTargets { purpose, legal, .. }) =
                pending.choice.as_mut()
            else {
                panic!("expected search choice")
            };
            let EffectTargetSelectionPurpose::SearchLibraryToHand {
                filter,
                filter_fingerprint,
                ..
            } = purpose
            else {
                panic!("expected search purpose")
            };
            match change {
                0 => *filter = LibraryCardFilter::InstantOrSorceryManaValue(2),
                1 => *filter_fingerprint ^= 1,
                2 => legal[0].target = Target::Object(opposing),
                _ => unreachable!(),
            }
            let before = serde_json::to_vec(&forged).unwrap();
            assert!(engine::step(&mut forged, Action::FinishEffectSelection).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
        }
    }
}

#[test]
fn restored_spell_search_rejects_coherent_artifact_filter_without_mutation() {
    use mtg_kernel::effect::{
        self, EffectFrame, EffectObjectBinding, EffectTargetCandidate,
        EffectTargetSelectionPurpose, LibraryCardFilter, PendingEffectChoice,
    };
    // FNV-1a for the merged artifact filter's tag 10 and mana value 1.
    const ARTIFACT_FINGERPRINT: u64 = 0x7f12_3281_14e3_0cce;
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = stage(player);
        let spell = put(&mut state, player, "Burst Lightning", Zone::Library);
        let artifact = put(&mut state, player, "Nihil Spellbomb", Zone::Library);
        assert_eq!(search(&mut state, player), vec![Target::Object(spell)]);
        let artifact_binding = EffectObjectBinding {
            object: artifact,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(artifact).zone_change_count,
        };
        let mut forged = restored(&state);
        let Some(PendingEffectChoice::SelectTargets { purpose, legal, .. }) = forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .choice
            .as_mut()
        else {
            panic!("expected the restored spell-search choice")
        };
        let EffectTargetSelectionPurpose::SearchLibraryToHand {
            filter,
            filter_fingerprint,
            ..
        } = purpose
        else {
            panic!("expected a library search")
        };
        *filter = LibraryCardFilter::ArtifactWithManaValue(1);
        *filter_fingerprint = ARTIFACT_FINGERPRINT;
        *legal = vec![EffectTargetCandidate {
            target: Target::Object(artifact),
            expected_object: Some(artifact_binding),
        }];
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(
            &mut forged,
            Action::ChooseEffectTarget(Target::Object(artifact))
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);

        // The unmodified restored prompt still accepts and resolves its spell.
        let mut valid = restored(&state);
        engine::step(
            &mut valid,
            Action::ChooseEffectTarget(Target::Object(spell)),
        )
        .unwrap();
        assert!(effect::validate_pending_effect_choice(&valid).is_ok());
        if matches!(next(&mut valid), Decision::ChooseEffectTargets { .. }) {
            engine::step(&mut valid, Action::FinishEffectSelection).unwrap();
        }
        assert!(matches!(
            settle(&mut valid),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(valid.objects.get(spell).zone, Zone::Hand);
        assert_eq!(valid.objects.get(artifact).zone, Zone::Library);

        // Answered frames must retain the same definition binding too.
        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        assert!(effect::validate_pending_effect_choice(&state).is_ok());
        let mut forged = restored(&state);
        let EffectFrame::SearchLibraryToHand {
            filter,
            filter_fingerprint,
            selected,
            ..
        } = &mut forged.engine.pending_effect.as_mut().unwrap().frames[0]
        else {
            panic!("expected an answered spell-search frame")
        };
        *filter = LibraryCardFilter::ArtifactWithManaValue(1);
        *filter_fingerprint = ARTIFACT_FINGERPRINT;
        *selected = Some(artifact_binding);
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);
    }
}

#[test]
fn coherent_search_forgeries_and_answered_program_edits_refuse() {
    use mtg_kernel::effect::{
        self, EffectFrame, EffectObjectBinding, EffectOp, EffectTargetCandidate,
        EffectTargetSelectionPurpose, LibraryCardFilter, PendingEffectChoice, PlayerRef,
    };
    // The existing tag-7 representation, independently recomputed so the
    // forged filter is internally consistent rather than a stale checksum.
    let fingerprint = [7_u64, 2]
        .into_iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, value| {
            value.to_le_bytes().into_iter().fold(hash, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
        });
    for player in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _) = stage(player);
        let mut forged = restored(&state);
        let Some(PendingEffectChoice::ChooseOption { options, .. }) = forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .choice
            .as_mut()
        else {
            panic!("expected optional choice")
        };
        options[0] = EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::AnyCard,
        };
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(&mut forged, Action::ChooseEffectOption(0)).is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);

        let mut answered = restored(&state);
        engine::step(&mut answered, Action::ChooseEffectOption(0)).unwrap();
        assert!(effect::validate_pending_effect_choice(&answered).is_ok());
        let EffectFrame::Program { op, .. } =
            &mut answered.engine.pending_effect.as_mut().unwrap().frames[0]
        else {
            panic!("expected answered option")
        };
        *op = EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::AnyCard,
        };
        assert!(effect::validate_pending_effect_choice(&answered).is_err());

        put(&mut state, player, "Burst Lightning", Zone::Library);
        let other_value = put(&mut state, player, "Think Twice", Zone::Library);
        assert_eq!(search(&mut state, player).len(), 1);
        let binding = EffectObjectBinding {
            object: other_value,
            expected_zone: Zone::Library,
            expected_zone_change_count: state.objects.get(other_value).zone_change_count,
        };
        let mut forged = restored(&state);
        let Some(PendingEffectChoice::SelectTargets { purpose, legal, .. }) = forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .choice
            .as_mut()
        else {
            panic!("expected search")
        };
        let EffectTargetSelectionPurpose::SearchLibraryToHand {
            filter,
            filter_fingerprint,
            ..
        } = purpose
        else {
            panic!("expected search purpose")
        };
        *filter = LibraryCardFilter::InstantOrSorceryManaValue(2);
        *filter_fingerprint = fingerprint;
        *legal = vec![EffectTargetCandidate {
            target: Target::Object(other_value),
            expected_object: Some(binding),
        }];
        let before = serde_json::to_vec(&forged).unwrap();
        assert!(engine::step(
            &mut forged,
            Action::ChooseEffectTarget(Target::Object(other_value))
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&forged).unwrap(), before);

        engine::step(&mut state, Action::FinishEffectSelection).unwrap();
        assert!(effect::validate_pending_effect_choice(&state).is_ok());
        let mut forged = restored(&state);
        let EffectFrame::SearchLibraryToHand {
            filter,
            filter_fingerprint,
            selected,
            ..
        } = &mut forged.engine.pending_effect.as_mut().unwrap().frames[0]
        else {
            panic!("expected answered search")
        };
        *filter = LibraryCardFilter::InstantOrSorceryManaValue(2);
        *filter_fingerprint = fingerprint;
        *selected = Some(binding);
        assert!(effect::validate_pending_effect_choice(&forged).is_err());

        let mut forged = restored(&state);
        forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .frames
            .clear();
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
        let mut forged = restored(&state);
        forged
            .engine
            .pending_effect
            .as_mut()
            .unwrap()
            .frames
            .push(EffectFrame::Program {
                op: EffectOp::Sequence(vec![]),
                path: vec![1],
            });
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
    }
}
