//! MageZero Standard family D: newer set keywords (prowess, valiant, speed,
//! warp, offspring, plot, crime and friends) in the Standard catalog.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

/// A main-phase state with `library` cards in both libraries, priority to P0.
fn ready_with(library: &str) -> GameState {
    let card = card_id_by_name(library).unwrap();
    let name = library.to_owned();
    let mut state =
        GameState::new_from_libraries(&[card; 30], &[card; 30], move |_| name.clone(), 0x4644);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn ready() -> GameState {
    ready_with("Mountain")
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn add_mana(state: &mut GameState, player: PlayerId, colored: &[ManaColor], generic: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[5] += generic;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..100 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    for target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                engine::step(state, Action::ChooseTarget(*target)).unwrap();
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
}

#[test]
fn chandra_copies_adventure_after_the_original_is_countered() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Chandra, Hope's Beacon",
        Zone::Battlefield,
    );
    let druid = put(&mut state, PlayerId::P0, "Questing Druid", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 1);
    cast(&mut state, druid, &[]);
    next(&mut state);
    assert_eq!(
        state.stack[0].v4.cast_method,
        Some(mtg_kernel::state::CastMethodV4::Omen)
    );
    assert_eq!(state.stack.len(), 2);
    state.priority_player = PlayerId::P1;
    let negate = put(&mut state, PlayerId::P1, "Negate", Zone::Hand);
    add_mana(&mut state, PlayerId::P1, &[ManaColor::U], 1);
    cast(&mut state, negate, &[Target::Object(druid)]);
    let mut restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    for state in [&mut state, &mut restored] {
        settled(state);
        assert_eq!(state.objects.get(druid).zone, Zone::Graveyard);
        assert_eq!(state.exile.len(), 2, "the copy resolves Seek the Beast");
        assert_eq!(state.engine.exile_play_permissions.len(), 2);
        assert!(state
            .objects
            .iter()
            .any(|(_, object)| object.spell_copy_origin.is_some()));
    }
    assert_eq!(state, restored);
}

#[test]
fn chandra_copies_omen_as_omen_and_both_spells_search() {
    let mut state = ready_with("Forest");
    put(
        &mut state,
        PlayerId::P0,
        "Chandra, Hope's Beacon",
        Zone::Battlefield,
    );
    let sagu = put(&mut state, PlayerId::P0, "Sagu Wildling", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::G], 0);
    cast(&mut state, sagu, &[]);
    next(&mut state);
    assert_eq!(state.stack.len(), 2);
    let mut searches = 0;
    let mut saw_omen_copy = false;
    for _ in 0..50 {
        match next(&mut state) {
            Decision::ChooseEffectTargets { .. } => {
                searches += 1;
                engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            }
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => break,
            Decision::CastSpellOrPass { .. } => {
                saw_omen_copy |= state.stack.iter().any(|item| {
                    item.is_copy
                        && item.v4.cast_method == Some(mtg_kernel::state::CastMethodV4::Omen)
                });
                engine::step(&mut state, Action::Pass).unwrap();
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(saw_omen_copy);
    assert_eq!(searches, 2);
    assert!(state.players[0].library.contains(&sagu));
    assert!(state.stack.is_empty());
}

#[test]
fn chandra_allows_retargeting_each_target_of_a_two_target_spell() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Chandra, Hope's Beacon",
        Zone::Battlefield,
    );
    let first = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let second = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let third = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let fourth = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let fire = put(&mut state, PlayerId::P0, "Cast into the Fire", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 1);
    engine::step(&mut state, Action::CastSpell(fire)).unwrap();
    if matches!(next(&mut state), Decision::ChooseSpellMode { .. }) {
        engine::step(&mut state, Action::ChooseSpellMode(0)).unwrap();
    }
    for target in [first, second] {
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(third)),
    )
    .unwrap();
    assert!(matches!(
        next(&mut state),
        Decision::ChooseEffectTargets { .. }
    ));
    let mut restored: GameState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    for state in [&mut state, &mut restored] {
        engine::step(state, Action::ChooseEffectTarget(Target::Object(fourth))).unwrap();
        settled(state);
        for target in [first, second, third, fourth] {
            assert_eq!(state.objects.get(target).zone, Zone::Graveyard);
        }
    }
    assert_eq!(state, restored);
}

#[test]
fn cage_relation_retains_public_source_after_cage_leaves_without_revealing_card() {
    let mut state = ready();
    let hidden = state.players[0].library[0];
    let cage = put(&mut state, PlayerId::P0, "Collector's Cage", Zone::Hand);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::W], 1);
    cast(&mut state, cage, &[]);
    assert!(matches!(
        settle(&mut state),
        Some(Decision::ChooseEffectTargets { .. })
    ));
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(hidden)),
    )
    .unwrap();
    settled(&mut state);
    let generation = state.objects.get(cage).zone_change_count;
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    for departed in [false, true] {
        if departed {
            mtg_kernel::event::propose_and_commit(
                &mut state,
                mtg_kernel::event::ProposedEvent::zone_change(cage, Zone::Graveyard),
            );
            settled(&mut state);
        }
        let view = mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P1, 0).unwrap();
        assert!(view.known_face_down_cards.is_empty());
        let bytes = serde_json::to_value(&view).unwrap();
        let relation = bytes["projection"]["object_relations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|relation| relation["object"]["arena_id"] == hidden.0)
            .unwrap();
        assert_eq!(relation["exiled_by"]["arena_id"], cage.0);
        assert_eq!(relation["exiled_by"]["zone_change_count"], generation);
        assert_eq!(
            relation["object"]["card_db_id"],
            card_id_by_name("Face-down card").unwrap()
        );
    }
}

#[test]
fn an_aborted_face_down_announcement_does_not_reveal_the_returned_hand_card() {
    let mut state = ready();
    let bloom = put(
        &mut state,
        PlayerId::P0,
        "Flourishing Bloom-Kin",
        Zone::Hand,
    );
    add_mana(&mut state, PlayerId::P0, &[], 3);
    cast(&mut state, bloom, &[]);
    assert!(state.objects.get(bloom).v4.face_down_v1.is_some());
    // Exercise the failed-announcement rollback before it has committed either form.
    state.players[0].mana_pool = [0; 6];
    next(&mut state);
    assert_eq!(state.objects.get(bloom).zone, Zone::Hand);
    let surface = mtg_kernel::surface_v2::HarnessSurfaceV2::new();
    let view = mtg_kernel::rl::observe_v2(&state, &surface, PlayerId::P1, 0).unwrap();
    assert!(view.known_hand_cards[0].is_empty());
    assert!(!serde_json::to_string(&view)
        .unwrap()
        .contains("Flourishing Bloom-Kin"));
}
