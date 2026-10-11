//! FDN batch 5: simple triggers and graveyard spells, all built from
//! existing engine primitives plus a surveil activated-ability recipe.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, Supertype, TargetSpec, CARD_DEFS,
    KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

const FIRST_ID: usize = 322;

/// Name, mana cost as (colored pips, generic), colors, types, subtypes,
/// power/toughness, keywords and trigger count, in id order.
#[allow(clippy::type_complexity)]
const CARDS: [(
    &str,
    &[ManaColor],
    u8,
    &[ManaColor],
    &[CardType],
    &[Subtype],
    Option<(i16, i16)>,
    Keywords,
    usize,
); 5] = [
    (
        "Zombify",
        &[ManaColor::B],
        3,
        &[ManaColor::B],
        &[CardType::Sorcery],
        &[],
        None,
        Keywords::NONE,
        0,
    ),
    (
        "Rune-Scarred Demon",
        &[ManaColor::B, ManaColor::B],
        5,
        &[ManaColor::B],
        &[CardType::Creature],
        &[Subtype::Demon],
        Some((6, 6)),
        Keywords::FLYING,
        1,
    ),
    (
        "Tatyova, Benthic Druid",
        &[ManaColor::G, ManaColor::U],
        3,
        &[ManaColor::G, ManaColor::U],
        &[CardType::Creature],
        &[Subtype::Merfolk, Subtype::Druid],
        Some((3, 3)),
        Keywords::NONE,
        1,
    ),
    (
        "Thrill of Possibility",
        &[ManaColor::R],
        1,
        &[ManaColor::R],
        &[CardType::Instant],
        &[],
        None,
        Keywords::NONE,
        0,
    ),
    (
        "Rune-Sealed Wall",
        &[ManaColor::U],
        2,
        &[ManaColor::U],
        &[CardType::Artifact, CardType::Creature],
        &[Subtype::Wall],
        Some((0, 6)),
        Keywords::DEFENDER,
        0,
    ),
];

fn ready(step: Step) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = step;
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
        summoning_sick: zone == Zone::Hand,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Library => state.players[player.index()].library.insert(0, id),
        _ => panic!("helper zone"),
    }
    id
}

fn mana(cost: &[(ManaColor, u8)], generic: u8) -> [u8; 6] {
    let mut pool = [0; 6];
    for &(color, count) in cost {
        pool[color.pool_index()] += count;
    }
    pool[5] += generic;
    pool
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

fn cast(state: &mut GameState, spell: ObjectId, target: Option<ObjectId>) {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)),
        "{} not castable",
        state.objects.get(spell).name
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    let after = next(state);
    assert!(
        matches!(after, Decision::CastSpellOrPass { .. }),
        "{after:?}"
    );
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    println!("FDN catalog hash: {KERNEL_CARDDB_HASH:016x}");
    for (offset, (name, pips, generic, colors, types, subtypes, stats, keywords, triggers)) in
        CARDS.into_iter().enumerate()
    {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, FIRST_ID + offset, "{name}");
        let def = &CARD_DEFS[id];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert_eq!(def.cost.generic, generic, "{name}");
        assert_eq!(def.cost.pips.len(), pips.len(), "{name}");
        assert_eq!(
            def.mana_value,
            u16::from(generic) + pips.len() as u16,
            "{name}"
        );
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.types, types, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            (stats.map(|s| s.0), stats.map(|s| s.1)),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
        assert_eq!(trigger::triggers_for(id as u16).len(), triggers, "{name}");
    }
    let def = |name| &CARD_DEFS[card_id_by_name(name).unwrap() as usize];
    assert_eq!(
        def("Zombify").target_spec,
        TargetSpec::CreatureCardInOwnGraveyard
    );
    assert!(def("Tatyova, Benthic Druid")
        .supertypes
        .contains(&Supertype::Legendary));
    assert_eq!(def("Rune-Sealed Wall").activated_abilities.len(), 1);
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Demon));
    assert!(Subtype::Demon.is_creature_type());
}

#[test]
fn reference_deck_resolves_all_forty_copies() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Zombify","count":1},
            {"name":"Rune-Scarred Demon","count":1},
            {"name":"Tatyova, Benthic Druid","count":1},
            {"name":"Thrill of Possibility","count":1},
            {"name":"Rune-Sealed Wall","count":1},
            {"name":"Island","count":35}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    let first = FIRST_ID as u16;
    assert_eq!(&ids[..5], &(first..first + 5).collect::<Vec<u16>>()[..]);
}

#[test]
fn zombify_returns_a_creature_card_from_its_controllers_graveyard() {
    let mut state = ready(Step::Main1);
    let demon = put(
        &mut state,
        PlayerId::P0,
        "Rune-Scarred Demon",
        Zone::Graveyard,
    );
    let noncreature = put(&mut state, PlayerId::P0, "Stab", Zone::Graveyard);
    let theirs = put(&mut state, PlayerId::P1, "Crypt Feaster", Zone::Graveyard);
    let zombify = put(&mut state, PlayerId::P0, "Zombify", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 3);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&zombify))
    );
    engine::step(&mut state, Action::CastSpell(zombify)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("expected a graveyard target");
    };
    assert_eq!(legal_targets, vec![Target::Object(demon)]);
    assert!(!legal_targets.contains(&Target::Object(noncreature)));
    assert!(!legal_targets.contains(&Target::Object(theirs)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(demon))).unwrap();
    // The returned Demon's enters trigger searches the library.
    let Some(Decision::ChooseEffectTargets { .. }) = settle(&mut state) else {
        panic!("expected the Demon's library search");
    };
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(demon).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(demon).controller, PlayerId::P0);
    assert_eq!(state.objects.get(zombify).zone, Zone::Graveyard);

    // With no creature card in its controller's graveyard it cannot be cast.
    let mut state = ready(Step::Main1);
    put(&mut state, PlayerId::P1, "Crypt Feaster", Zone::Graveyard);
    let zombify = put(&mut state, PlayerId::P0, "Zombify", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 1)], 3);
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&zombify))
    );
}

#[test]
fn rune_scarred_demon_tutors_any_card_into_hand_unrevealed() {
    let mut state = ready(Step::Main1);
    let stab = put(&mut state, PlayerId::P0, "Stab", Zone::Library);
    let demon = put(&mut state, PlayerId::P0, "Rune-Scarred Demon", Zone::Hand);
    state.players[0].mana_pool = mana(&[(ManaColor::B, 2)], 5);
    cast(&mut state, demon, None);
    let Some(Decision::ChooseEffectTargets {
        player,
        legal_targets,
        min_targets,
        max_targets,
        can_finish,
        ..
    }) = settle(&mut state)
    else {
        panic!("expected the library search");
    };
    assert_eq!(
        (player, min_targets, max_targets, can_finish),
        (PlayerId::P0, 0, 1, true)
    );
    assert_eq!(legal_targets.len(), state.players[0].library.len());
    assert!(legal_targets.contains(&Target::Object(stab)));
    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(stab))).unwrap();
    settled(&mut state);
    assert_eq!(state.objects.get(demon).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(stab).zone, Zone::Hand);
    assert!(state
        .known_hand_cards(PlayerId::P1, PlayerId::P0)
        .is_empty());
}

#[test]
fn tatyova_gains_life_and_draws_for_each_land_its_controller_plays() {
    let mut state = ready(Step::Main1);
    let tatyova = put(
        &mut state,
        PlayerId::P0,
        "Tatyova, Benthic Druid",
        Zone::Battlefield,
    );
    let island = put(&mut state, PlayerId::P0, "Island", Zone::Hand);
    let hand = state.players[0].hand.len();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { land_drops, .. } if land_drops.contains(&island))
    );
    engine::step(&mut state, Action::PlayLand(island)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].life, 21);
    // The land left the hand and one card was drawn.
    assert_eq!(state.players[0].hand.len(), hand);
    assert_eq!(state.objects.get(tatyova).zone, Zone::Battlefield);

    // An opponent's land does not trigger it.
    let theirs = put(&mut state, PlayerId::P1, "Island", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(theirs, Zone::Battlefield),
    );
    let triggers = trigger::collect_and_process(&mut state);
    assert!(triggers.is_empty());
}

#[test]
fn thrill_of_possibility_discards_as_a_cost_then_draws_two() {
    let mut state = ready(Step::Main1);
    let kept = put(&mut state, PlayerId::P0, "Stab", Zone::Hand);
    let thrill = put(
        &mut state,
        PlayerId::P0,
        "Thrill of Possibility",
        Zone::Hand,
    );
    state.players[0].mana_pool = mana(&[(ManaColor::R, 1)], 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&thrill))
    );
    engine::step(&mut state, Action::CastSpell(thrill)).unwrap();
    // The only other card in hand pays the discard cost.
    match next(&mut state) {
        Decision::Discard { choices, .. } => {
            assert_eq!(choices, vec![kept]);
            engine::step(&mut state, Action::Discard(vec![kept])).unwrap();
        }
        Decision::CastSpellOrPass { .. } => {}
        other => panic!("unexpected cost decision: {other:?}"),
    }
    assert_eq!(state.objects.get(kept).zone, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(thrill).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2);

    // With no other card to discard it cannot be cast.
    let mut state = ready(Step::Main1);
    let thrill = put(
        &mut state,
        PlayerId::P0,
        "Thrill of Possibility",
        Zone::Hand,
    );
    state.players[0].mana_pool = mana(&[(ManaColor::R, 1)], 1);
    assert!(
        !matches!(next(&mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&thrill))
    );
}

#[test]
fn rune_sealed_wall_taps_to_surveil_one_and_cannot_attack() {
    let mut state = ready(Step::Main1);
    let wall = put(
        &mut state,
        PlayerId::P0,
        "Rune-Sealed Wall",
        Zone::Battlefield,
    );
    let top = state.players[0].library[0];
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.iter().any(|ability| ability.0 == wall))
    );
    engine::step(&mut state, Action::ActivateAbility(wall, 0)).unwrap();
    let Some(Decision::ChooseEffectOption {
        player,
        option_count,
        ..
    }) = settle(&mut state)
    else {
        panic!("expected surveil");
    };
    assert_eq!((player, option_count), (PlayerId::P0, 2));
    assert!(state.objects.get(wall).tapped);
    engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
    settled(&mut state);
    assert_eq!(state.players[0].graveyard, vec![top]);
    // Tapped, it cannot activate again this turn.
    assert!(engine::step(&mut state, Action::ActivateAbility(wall, 0)).is_err());

    let mut state = ready(Step::DeclareAttackers);
    let wall = put(
        &mut state,
        PlayerId::P0,
        "Rune-Sealed Wall",
        Zone::Battlefield,
    );
    assert!(engine::step(&mut state, Action::DeclareAttackers(vec![wall])).is_err());
}
