//! Focused Deep Analysis coverage for the generic target-player draw program
//! and ordered composite flashback-cost substrate.
//!
//! The bounded rules oracle is XMage commit
//! `0723fc0c2be922af47b0ef0539f28114cc23b998`: `DeepAnalysis.java` blob
//! `3b039ba444b5fb88b3a43b1fb32dc2164035a0e6`, `PayLifeCost.java` blob
//! `65ef04d0d86c2c8f037d9fc942a1bf687a431e34`, `FlashbackAbility.java` blob
//! `4b7dae1d7092ba316760f4ecb6077f6ebf12fd1e`, and
//! `DrawCardTargetEffect.java` blob
//! `210b127ae019fb64d47be3ae6c4c99ea74a1f7db`. Together they establish
//! `{3}{U}`, "target player draws two cards", and flashback `{1}{U}` plus
//! mandatory pay 3 life. Real AIRL trace
//! `game_20260714_011043_0004.txt` records the candidate as exactly
//! `Deep Analysis [c51]: Flashback {1}{U}`, then life 16 -> 13, two newly
//! tapped Islands, two draws, and exile; XMage's display omits the separately
//! added life cost even though payment is mandatory.

use mtg_kernel::card_def::{card_id_by_name, preflight_fully_supported_deck, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::rl::{legal_action_candidates_v1, observe_v2, ActionSemanticV1, PlayerSeatV1};
use mtg_kernel::state::{Counters, GameObject, GameState, StackItem, Step, Target, Zone};
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
        Zone::Stack => panic!("test helper does not construct stack objects"),
    }
    id
}

fn ready_deep(
    origin: Zone,
    life: i32,
    islands: usize,
    p0_library: &[&str],
    p1_library: &[&str],
) -> (GameState, ObjectId, Vec<ObjectId>, [Vec<ObjectId>; 2]) {
    assert!(matches!(origin, Zone::Hand | Zone::Graveyard));
    let p0_defs = p0_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let p1_defs = p1_library
        .iter()
        .map(|name| card_id(name))
        .collect::<Vec<_>>();
    let mut state =
        GameState::new_from_libraries(&p0_defs, &p1_defs, card_name, 0x4445_4550_414E_414C);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.players[0].life = life;
    let deep = put_object(&mut state, PlayerId::P0, "Deep Analysis", origin);
    let islands = (0..islands)
        .map(|_| put_object(&mut state, PlayerId::P0, "Island", Zone::Battlefield))
        .collect::<Vec<_>>();
    let libraries = [
        state.players[0].library.clone(),
        state.players[1].library.clone(),
    ];
    (state, deep, islands, libraries)
}

fn cast_decision(state: &mut GameState) -> Decision {
    engine::advance_until_decision(state)
}

fn is_offered(state: &GameState, deep: ObjectId) -> bool {
    matches!(
        cast_decision(&mut state.clone()),
        Decision::CastSpellOrPass { castable_spells, .. }
            if castable_spells.contains(&deep)
    )
}

fn cast_and_target(state: &mut GameState, deep: ObjectId, target: PlayerId) -> Decision {
    let offer = cast_decision(state);
    assert!(matches!(
        offer,
        Decision::CastSpellOrPass { ref castable_spells, .. }
            if castable_spells.contains(&deep)
    ));
    engine::step(state, Action::CastSpell(deep)).unwrap();
    let target_decision = engine::advance_until_decision(state);
    assert!(matches!(
        target_decision,
        Decision::ChooseTargets {
            player: PlayerId::P0,
            spell,
            remaining: 1,
            ref legal_targets,
        } if spell == deep
            && legal_targets == &vec![
                Target::Player(PlayerId::P0),
                Target::Player(PlayerId::P1),
            ]
    ));
    engine::step(state, Action::ChooseTarget(Target::Player(target))).unwrap();
    target_decision
}

fn pass_until_stack_len(state: &mut GameState, wanted: usize) -> Decision {
    for _ in 0..16 {
        let decision = engine::advance_until_decision(state);
        if state.stack.len() <= wanted {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::GameOver { .. } | Decision::Halted { .. } => return decision,
            other => panic!("unexpected decision while resolving Deep Analysis: {other:?}"),
        }
    }
    panic!("stack did not reach length {wanted}")
}

fn stable_ids(state: &GameState, decision: &Decision) -> Vec<String> {
    legal_action_candidates_v1(&SurfaceDecision::Decision(decision.clone()), state)
        .expect("schema-v4 legal action projection")
        .into_iter()
        .map(|candidate| candidate.record.stable_id)
        .collect()
}

fn draw_events_since(state: &GameState, start: usize, player: PlayerId) -> Vec<Option<ObjectId>> {
    state.engine.event_history[start..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Draw {
                player: event_player,
                object,
            } if *event_player == player => Some(*object),
            _ => None,
        })
        .collect()
}

#[test]
fn normal_cast_draws_two_for_self_or_opponent_without_paying_life() {
    for target in [PlayerId::P0, PlayerId::P1] {
        let (mut state, deep, islands, libraries) = ready_deep(
            Zone::Hand,
            2,
            4,
            &["Lightning Bolt", "Mountain", "Fireblast"],
            &["Fiery Temper", "Lava Dart", "Highway Robbery"],
        );
        let history_start = state.engine.event_history.len();
        cast_and_target(&mut state, deep, target);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }
        ));
        assert_eq!(state.players[0].life, 2, "normal cast has no life cost");
        assert!(islands.iter().all(|&id| state.objects.get(id).tapped));
        let decision = pass_until_stack_len(&mut state, 0);
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));

        assert_eq!(
            state.players[target.index()].hand,
            libraries[target.index()][..2]
        );
        assert_eq!(state.players[target.index()].draws_this_turn, 2);
        let other = if target == PlayerId::P0 {
            PlayerId::P1
        } else {
            PlayerId::P0
        };
        assert!(state.players[other.index()].hand.is_empty());
        assert_eq!(state.players[other.index()].draws_this_turn, 0);
        assert_eq!(state.objects.get(deep).zone, Zone::Graveyard);
        assert_eq!(
            draw_events_since(&state, history_start, target),
            libraries[target.index()][..2]
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn flashback_life_two_is_illegal_life_three_loses_and_life_four_resolves() {
    let libraries = ["Lightning Bolt", "Mountain", "Fireblast"];

    let (state, deep, _, _) = ready_deep(Zone::Graveyard, 2, 2, &libraries, &libraries);
    assert!(!is_offered(&state, deep));
    let before = state.clone();
    let mut rejected = state;
    assert!(engine::step(&mut rejected, Action::CastSpell(deep)).is_err());
    assert_eq!(
        rejected, before,
        "an unaffordable flashback cannot mutate state"
    );

    let (mut exact, deep, _, _) = ready_deep(Zone::Graveyard, 3, 2, &libraries, &libraries);
    assert!(is_offered(&exact, deep));
    cast_and_target(&mut exact, deep, PlayerId::P0);
    let exact_decision = engine::advance_until_decision(&mut exact);
    assert_eq!(exact.players[0].life, 0);
    assert_eq!(exact.objects.get(deep).zone, Zone::Stack);
    assert!(exact.stack.iter().any(|item| item.source == deep));
    assert!(
        matches!(
            &exact_decision,
            Decision::GameOver {
                winner: Some(PlayerId::P1)
            }
        ),
        "expected immediate SBA loss, got {exact_decision:?}"
    );
    assert!(
        exact.players[0].hand.is_empty(),
        "the spell never resolves after SBA loss"
    );
    assert_eq!(exact.objects.get(deep).zone, Zone::Stack);

    let (mut above, deep, islands, libraries) =
        ready_deep(Zone::Graveyard, 4, 2, &libraries, &libraries);
    let history_start = above.engine.event_history.len();
    cast_and_target(&mut above, deep, PlayerId::P0);
    assert!(matches!(
        engine::advance_until_decision(&mut above),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    assert_eq!(above.players[0].life, 1);
    assert!(islands.iter().all(|&id| above.objects.get(id).tapped));
    assert!(matches!(
        &above.engine.event_history[history_start..],
        [
            CommittedEvent::Tap { .. },
            CommittedEvent::ManaAdded { .. },
            CommittedEvent::Tap { .. },
            CommittedEvent::ManaAdded { .. },
            CommittedEvent::LifeLoss {
                player: PlayerId::P0,
                amount: 3,
            },
            CommittedEvent::SpellCast {
                controller: PlayerId::P0,
                ..
            },
        ]
    ));
    pass_until_stack_len(&mut above, 0);
    assert_eq!(above.players[0].hand, libraries[0][..2]);
    assert_eq!(above.objects.get(deep).zone, Zone::Exile);
    assert!(above.exile.contains(&deep));
}

#[test]
fn counter_exiles_physical_flashback_but_malformed_target_metadata_halts() {
    let cards = ["Lightning Bolt", "Mountain", "Fireblast"];
    let (mut countered, deep, _, _) = ready_deep(Zone::Graveyard, 4, 2, &cards, &cards);
    put_object(&mut countered, PlayerId::P1, "Island", Zone::Battlefield);
    put_object(&mut countered, PlayerId::P1, "Island", Zone::Battlefield);
    let counterspell = put_object(&mut countered, PlayerId::P1, "Counterspell", Zone::Hand);
    cast_and_target(&mut countered, deep, PlayerId::P0);
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    engine::step(&mut countered, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut countered, Action::CastSpell(counterspell)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut countered),
        Decision::ChooseTargets { spell, .. } if spell == counterspell
    ));
    engine::step(&mut countered, Action::ChooseTarget(Target::Object(deep))).unwrap();
    pass_until_stack_len(&mut countered, 0);
    assert_eq!(countered.players[0].life, 1, "cost remains paid");
    assert!(countered.players[0].hand.is_empty());
    assert_eq!(countered.objects.get(deep).zone, Zone::Exile);
    assert_eq!(countered.objects.get(counterspell).zone, Zone::Graveyard);

    let (mut fizzled, deep, _, _) = ready_deep(Zone::Graveyard, 4, 2, &cards, &cards);
    let history_start = fizzled.engine.event_history.len();
    cast_and_target(&mut fizzled, deep, PlayerId::P1);
    assert!(matches!(
        engine::advance_until_decision(&mut fizzled),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    fizzled
        .stack
        .last_mut()
        .expect("Deep Analysis on stack")
        .targets
        .clear();
    engine::step(&mut fizzled, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut fizzled),
        Decision::CastSpellOrPass {
            player: PlayerId::P1,
            ..
        }
    ));
    engine::step(&mut fizzled, Action::Pass).unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut fizzled),
        Decision::Halted {
            mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
            source,
        } if source == deep
    ));
    assert!(draw_events_since(&fizzled, history_start, PlayerId::P1).is_empty());
    assert_eq!(fizzled.objects.get(deep).zone, Zone::Stack);
    assert_eq!(fizzled.stack.last().map(|item| item.source), Some(deep));
}

#[test]
fn copied_flashback_draws_without_repaying_and_virtual_copy_never_enters_exile() {
    let cards = ["Lightning Bolt", "Mountain", "Fireblast", "Lava Dart"];
    let (mut state, deep, _, libraries) = ready_deep(Zone::Graveyard, 4, 2, &cards, &cards);
    cast_and_target(&mut state, deep, PlayerId::P1);
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    let parent = state.stack.last().expect("physical Deep Analysis").clone();
    assert!(parent.is_flashback);
    let original = state.objects.get(deep).clone();
    let copy_source = state.objects.push(GameObject {
        card_def: original.card_def,
        name: original.name,
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone: Zone::Stack,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: mtg_kernel::state::ObjectStateV4::from_card_def(original.card_def),
        spell_copy_origin: Some(mtg_kernel::state::SpellCopyOriginV4 {
            parent: deep,
            parent_card_def: original.card_def,
            parent_owner: original.owner,
            parent_controller: original.controller,
            parent_stack_zone_change_count: original.zone_change_count,
            parent_was_copy: false,
        }),
        plotted_turn: None,
        zone_change_count: 0,
    });
    let mut copy: StackItem = parent;
    copy.source = copy_source;
    copy.is_copy = true;
    copy.is_flashback = false;
    copy.v4.cast_method = Some(mtg_kernel::state::CastMethodV4::Normal);
    copy.v4.source_contract = Some(mtg_kernel::state::StackSourceContractV4::capture(
        &state,
        copy_source,
        mtg_kernel::state::CastMethodV4::Normal,
    ));
    state.stack.push(copy);
    let history_start = state.engine.event_history.len();

    pass_until_stack_len(&mut state, 1);
    assert_eq!(state.players[0].life, 1, "only the physical cast paid life");
    assert_eq!(state.players[1].hand, libraries[1][..2]);
    assert_eq!(state.objects.get(copy_source).zone, Zone::Stack);
    assert!(!state.exile.contains(&copy_source));
    assert!(!state.players[0].graveyard.contains(&copy_source));
    assert!(!state.engine.event_history[history_start..]
        .iter()
        .any(|event| {
            matches!(event, CommittedEvent::ZoneChange { object, .. } if *object == copy_source)
        }));
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].source, deep);
}

#[test]
fn two_empty_draw_attempts_finish_flashback_exile_before_game_loss() {
    let cards = ["Lightning Bolt", "Mountain", "Fireblast"];
    let (mut state, deep, _, _) = ready_deep(Zone::Graveyard, 4, 2, &cards, &[]);
    let history_start = state.engine.event_history.len();
    cast_and_target(&mut state, deep, PlayerId::P1);
    let decision = pass_until_stack_len(&mut state, 0);
    assert!(matches!(
        decision,
        Decision::GameOver {
            winner: Some(PlayerId::P0)
        }
    ));
    assert_eq!(
        draw_events_since(&state, history_start, PlayerId::P1),
        vec![None, None],
        "DrawCards(2) proposes both individual draw events before SBA"
    );
    assert!(state.players[1].drew_from_empty);
    assert!(state.players[1].has_lost);
    assert_eq!(state.objects.get(deep).zone, Zone::Exile);
}

#[test]
fn schema_v4_cast_and_target_actions_are_snapshot_stable() {
    let cards = ["Lightning Bolt", "Mountain", "Fireblast"];
    let (mut state, deep, _, _) = ready_deep(Zone::Graveyard, 4, 2, &cards, &cards);
    preflight_fully_supported_deck(&[state.objects.get(deep).card_def]).unwrap();

    let cast = cast_decision(&mut state);
    let cast_candidates =
        legal_action_candidates_v1(&SurfaceDecision::Decision(cast.clone()), &state).unwrap();
    let deep_cast = cast_candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.record.semantic,
                ActionSemanticV1::CastSpell {
                    actor: PlayerSeatV1::P0,
                    ref source,
                } if source.arena_id == deep.0
            )
        })
        .expect("Deep Analysis flashback action")
        .record
        .stable_id
        .clone();
    assert_eq!(deep_cast, "legal-action-v4:bc34f038c6c09b4e");
    let cast_snapshot = state.snapshot();
    let cast_hash = state.state_hash();
    state.restore(&cast_snapshot);
    assert_eq!(state.state_hash(), cast_hash);
    let restored_cast = cast_decision(&mut state);
    assert!(stable_ids(&state, &restored_cast).contains(&deep_cast));

    engine::step(&mut state, Action::CastSpell(deep)).unwrap();
    let target = engine::advance_until_decision(&mut state);
    let target_ids = stable_ids(&state, &target);
    assert_eq!(
        target_ids,
        [
            "legal-action-v4:cf57b3e563eeda8f",
            "legal-action-v4:0c6cbddcf9da4b1e",
        ]
    );
    let target_snapshot = state.snapshot();
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass {
            player: PlayerId::P0,
            ..
        }
    ));
    let expected_hash = state.state_hash();
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    let stack_item = observation
        .projection
        .stack
        .last()
        .expect("Deep Analysis stack observation");
    assert!(stack_item.is_flashback);
    assert_eq!(
        stack_item.cast_method,
        Some(mtg_kernel::state::CastMethodV4::Flashback)
    );

    state.restore(&target_snapshot);
    let restored_target = engine::advance_until_decision(&mut state);
    assert_eq!(stable_ids(&state, &restored_target), target_ids);
    engine::step(
        &mut state,
        Action::ChooseTarget(Target::Player(PlayerId::P1)),
    )
    .unwrap();
    let _ = engine::advance_until_decision(&mut state);
    assert_eq!(state.state_hash(), expected_hash);
}
