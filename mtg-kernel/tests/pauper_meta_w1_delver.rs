//! Delver of Secrets: an in-place transform (no zone change, same
//! `ObjectId`, same `zone_change_count`) driven by a controller-only
//! beginning-of-upkeep trigger that looks privately at the top of the
//! library and offers a public "may reveal" decision.
//!
//! The bounded rules oracle is the Mage fork at `72a08a3b`:
//! `DelverOfSecrets.java` blob `1006315827d5a9619823e17c11fb49773cf91642`.
//! It establishes: at the beginning of your upkeep, look at the top card of
//! your library. You may reveal that card. If an instant or sorcery card is
//! revealed this way, transform Delver of Secrets. Insectile Aberration is
//! a 3/2 Human Insect, blue, with flying.

use mtg_kernel::card_def::{card_id_by_name, mana_colors_mask, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

// Copied verbatim from `tests/pauper_meta_w1_creatures.rs`.
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
        v4: ObjectStateV4::from_card_def(card_def),
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

/// Places an already-transformed Delver of Secrets (`face_index` 1,
/// Insectile Aberration) directly on P0's battlefield, with the same
/// `v4.effective_color_mask`/`v4.effective_subtype_ids`/`name` fields
/// `event::commit`'s `ProposedEvent::Transform` arm would have set. This
/// file's other tests exercise the transform itself; this helper starts
/// from its far side, to check nothing re-triggers it.
fn put_transformed_delver(state: &mut GameState) -> ObjectId {
    let card_def = card_id("Delver of Secrets");
    let def = &CARD_DEFS[card_def as usize];
    let face = def
        .transform_face
        .as_ref()
        .expect("Delver of Secrets has a transform_face");
    let mut v4 = ObjectStateV4::from_card_def(card_def);
    v4.face_index = 1;
    v4.effective_color_mask = mana_colors_mask(face.colors);
    v4.effective_subtype_ids = {
        let mut ids: Vec<u16> = face.subtypes.iter().map(|s| s.stable_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let id = state.objects.push(GameObject {
        card_def,
        name: face.name.to_string(),
        owner: PlayerId::P0,
        controller: PlayerId::P0,
        zone: Zone::Battlefield,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4,
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[0].battlefield.push(id);
    id
}

/// Starts a fresh game at `Step::Untap` on turn 1
/// (`GameState::new_from_libraries`'s own default), `active_player`
/// already holding priority as usual, with Delver of Secrets controlled by
/// P0 already on P0's battlefield and, if given, `top_of_library` seeded as
/// the sole card in `active_player`'s library (so it sits on top).
fn ready_before_upkeep(active_player: PlayerId, top_of_library: Option<&str>) -> (GameState, ObjectId) {
    let library: Vec<u16> = top_of_library.into_iter().map(card_id).collect();
    let (p0_lib, p1_lib): (&[u16], &[u16]) = if active_player == PlayerId::P0 {
        (&library, &[])
    } else {
        (&[], &library)
    };
    let mut state = GameState::new_from_libraries(p0_lib, p1_lib, card_name, 1);
    state.active_player = active_player;
    state.priority_player = active_player;
    let delver = put_object(&mut state, PlayerId::P0, "Delver of Secrets", Zone::Battlefield);
    (state, delver)
}

/// Passes at every `CastSpellOrPass` decision until a different decision (or
/// a terminal one) is reached, bounded so a wiring bug fails the test
/// instead of hanging it. Delver's own reveal decision
/// (`Decision::ChooseEffectBoolean`) is exactly such a "different decision."
fn pass_to_next_decision(state: &mut GameState) -> Decision {
    for _ in 0..16 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
    panic!("a non-priority decision was not reached within the bounded priority walk");
}

/// Passes at every `CastSpellOrPass` decision until the stack is empty,
/// bounded the same way as `pass_to_next_decision`. Used after answering
/// the reveal decision to let its (possibly empty) continuation finish
/// resolving before inspecting the resulting state.
fn pass_to_stack_empty(state: &mut GameState) {
    for _ in 0..16 {
        match engine::advance_until_decision(state) {
            Decision::CastSpellOrPass { .. } => {
                if state.stack.is_empty() {
                    return;
                }
                engine::step(state, Action::Pass).unwrap();
            }
            other => panic!("unexpected decision while resolving the stack: {other:?}"),
        }
    }
    panic!("the stack did not empty within the bounded priority walk");
}

#[test]
fn delver_transforms_when_the_revealed_top_card_is_an_instant_or_sorcery() {
    let (state, delver) = ready_before_upkeep(PlayerId::P0, Some("Ponder"));
    let original_zone_change_count = state.objects.get(delver).zone_change_count;

    // Accept the reveal: Ponder is a sorcery, so Delver transforms.
    let mut accepted = state.clone();
    let decision = pass_to_next_decision(&mut accepted);
    match &decision {
        Decision::ChooseEffectBoolean {
            player,
            source,
            default,
            ..
        } => {
            assert_eq!(*player, PlayerId::P0);
            assert_eq!(*source, delver);
            assert_eq!(*default, Some(false), "declining the reveal is always legal");
        }
        other => panic!("expected the reveal ChooseEffectBoolean, got {other:?}"),
    }
    // Private look, public reveal: before the reveal decision is answered,
    // the opponent's view of the controller's library must still be empty
    // (the initial look is `reveal_library_top(controller, controller, 1)`,
    // recorded only in the controller's own row), while the controller's
    // own view is already populated.
    assert!(
        accepted
            .known_library_cards(PlayerId::P1, PlayerId::P0)
            .is_empty(),
        "the opponent must not learn the top card from a private look"
    );
    assert!(
        !accepted
            .known_library_cards(PlayerId::P0, PlayerId::P0)
            .is_empty(),
        "the controller privately knows the top card after looking"
    );
    engine::step(&mut accepted, Action::ChooseEffectBoolean(true)).unwrap();
    pass_to_stack_empty(&mut accepted);

    assert!(
        !accepted
            .known_library_cards(PlayerId::P1, PlayerId::P0)
            .is_empty(),
        "an accepted reveal is public: the opponent now knows the top card too"
    );

    let object = accepted.objects.get(delver);
    assert_eq!(object.name, "Insectile Aberration");
    assert_eq!(object.v4.face_index, 1);
    assert_eq!(object.zone, Zone::Battlefield, "no zone change");
    assert_eq!(
        object.zone_change_count, original_zone_change_count,
        "in-place transform: zone_change_count is unchanged"
    );
    assert_eq!(engine::effective_power(&accepted, delver), 3);
    assert_eq!(engine::effective_toughness(&accepted, delver), 2);
    assert!(engine::has_effective_keyword(&accepted, delver, Keywords::FLYING));

    // Decline the reveal from the identical starting point: no transform,
    // even though Ponder was privately known (to the controller only) to be
    // a sorcery.
    let mut declined = state;
    pass_to_next_decision(&mut declined);
    engine::step(&mut declined, Action::ChooseEffectBoolean(false)).unwrap();
    pass_to_stack_empty(&mut declined);

    let object = declined.objects.get(delver);
    assert_eq!(object.name, "Delver of Secrets");
    assert_eq!(object.v4.face_index, 0);
    assert_eq!(object.zone_change_count, original_zone_change_count);
    assert_eq!(engine::effective_power(&declined, delver), 1);
    assert_eq!(engine::effective_toughness(&declined, delver), 1);
    assert!(!engine::has_effective_keyword(&declined, delver, Keywords::FLYING));
}

#[test]
fn delver_does_not_transform_on_a_land_or_creature() {
    let (state, delver) = ready_before_upkeep(PlayerId::P0, Some("Island"));
    let original_zone_change_count = state.objects.get(delver).zone_change_count;

    let mut accepted = state;
    let decision = pass_to_next_decision(&mut accepted);
    assert!(
        matches!(decision, Decision::ChooseEffectBoolean { .. }),
        "expected the reveal ChooseEffectBoolean, got {decision:?}"
    );
    engine::step(&mut accepted, Action::ChooseEffectBoolean(true)).unwrap();
    pass_to_stack_empty(&mut accepted);

    let object = accepted.objects.get(delver);
    assert_eq!(
        object.name, "Delver of Secrets",
        "Island is a land, not an instant or sorcery"
    );
    assert_eq!(object.v4.face_index, 0);
    assert_eq!(object.zone_change_count, original_zone_change_count);
    assert_eq!(engine::effective_power(&accepted, delver), 1);
    assert_eq!(engine::effective_toughness(&accepted, delver), 1);
}

#[test]
fn delver_trigger_is_controllers_upkeep_only() {
    let (mut state, delver) = ready_before_upkeep(PlayerId::P1, None);
    let original_zone_change_count = state.objects.get(delver).zone_change_count;

    // Delver is controlled by P0, but it's P1's upkeep (P1 is the active
    // player): the controller-only trigger must not fire, so the very
    // first priority window opens with an empty stack, never a
    // `ChooseEffectBoolean`.
    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass { player, .. } => {
            assert_eq!(player, PlayerId::P1, "priority opens with the active player");
            assert!(
                state.stack.is_empty(),
                "Delver's controller-only trigger must not fire on a non-controller's upkeep"
            );
        }
        other => panic!("unexpected decision on the non-controller's upkeep: {other:?}"),
    }

    let object = state.objects.get(delver);
    assert_eq!(object.name, "Delver of Secrets");
    assert_eq!(object.v4.face_index, 0);
    assert_eq!(object.zone_change_count, original_zone_change_count);
}

#[test]
fn delver_does_not_retrigger_once_transformed() {
    // DelverOfSecrets.java attaches the upkeep trigger to the front half
    // only (`getLeftHalfCard().addAbility(...)`); Insectile Aberration
    // carries none. An already-transformed Delver with another instant on
    // top of its controller's library must not look, must not offer a
    // reveal decision, and must not transform again (which would otherwise
    // run `TransformSourceInPlace` against `face_index == 1` and halt the
    // engine, per its own guard).
    let top = card_id("Lightning Bolt");
    let mut state = GameState::new_from_libraries(&[top], &[], card_name, 1);
    let delver = put_transformed_delver(&mut state);
    let original_zone_change_count = state.objects.get(delver).zone_change_count;

    match engine::advance_until_decision(&mut state) {
        Decision::CastSpellOrPass { player, .. } => {
            assert_eq!(player, PlayerId::P0, "priority opens with the active player");
            assert!(
                state.stack.is_empty(),
                "a transformed Delver's front-face trigger must not fire again"
            );
        }
        other => panic!("unexpected decision at the transformed Delver's upkeep: {other:?}"),
    }
    assert!(
        state.engine.halted.is_none(),
        "the engine must not halt: got {:?}",
        state.engine.halted
    );

    let object = state.objects.get(delver);
    assert_eq!(object.name, "Insectile Aberration");
    assert_eq!(object.v4.face_index, 1);
    assert_eq!(object.zone, Zone::Battlefield);
    assert_eq!(object.zone_change_count, original_zone_change_count);
}
