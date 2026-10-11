//! FDN gainlands and life-gain/drain creatures: definitions and trigger rules.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;

const GAINLANDS: [(&str, [ManaColor; 2]); 8] = [
    ("Bloodfell Caves", [ManaColor::B, ManaColor::R]),
    ("Dismal Backwater", [ManaColor::U, ManaColor::B]),
    ("Jungle Hollow", [ManaColor::B, ManaColor::G]),
    ("Rugged Highlands", [ManaColor::R, ManaColor::G]),
    ("Scoured Barrens", [ManaColor::W, ManaColor::B]),
    ("Swiftwater Cliffs", [ManaColor::U, ManaColor::R]),
    ("Tranquil Cove", [ManaColor::W, ManaColor::U]),
    ("Wind-Scarred Crag", [ManaColor::R, ManaColor::W]),
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
        _ => panic!("helper zone"),
    }
    id
}

/// Puts a land onto the battlefield from hand and queues its triggers.
fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
    let triggers = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(triggers);
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

/// Orders and resolves every pending trigger until priority returns with an
/// empty stack.
fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        match next(surface, state) {
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("trigger drain did not finish");
}

fn life(state: &GameState) -> (i32, i32) {
    (state.players[0].life, state.players[1].life)
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    let first = card_id_by_name("Bloodfell Caves").unwrap() as usize;
    assert_eq!(first, 243);
    for (offset, (name, colors)) in GAINLANDS.into_iter().enumerate() {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, first + offset, "{name}");
        let def = &CARD_DEFS[id];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert!(def.is_land, "{name}");
        assert_eq!(def.produces_mana, &colors, "{name}");
        assert!(def.colors.is_empty(), "{name}");
        assert_eq!(trigger::triggers_for(id as u16).len(), 1, "{name}");
    }
    for (offset, (name, mana_value, colors, subtypes, stats)) in [
        (
            "Ajani's Pridemate",
            2,
            &[ManaColor::W][..],
            &[Subtype::Cat, Subtype::Soldier][..],
            (2, 2),
        ),
        (
            "Marauding Blight-Priest",
            3,
            &[ManaColor::B][..],
            &[Subtype::Vampire, Subtype::Cleric][..],
            (3, 2),
        ),
        (
            "Sanguine Syphoner",
            2,
            &[ManaColor::B][..],
            &[Subtype::Vampire, Subtype::Warlock][..],
            (1, 3),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, 251 + offset, "{name}");
        let def = &CARD_DEFS[id];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert_eq!(def.mana_value, mana_value, "{name}");
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!((def.power, def.toughness), (Some(stats.0), Some(stats.1)));
        assert_eq!(def.keywords, Keywords::NONE, "{name}");
        assert_eq!(trigger::triggers_for(id as u16).len(), 1, "{name}");
    }
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Warlock));
}

#[test]
fn reference_deck_resolves_all_forty_copies() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Bloodfell Caves","count":1},{"name":"Dismal Backwater","count":1},
            {"name":"Jungle Hollow","count":1},{"name":"Rugged Highlands","count":1},
            {"name":"Scoured Barrens","count":1},{"name":"Swiftwater Cliffs","count":1},
            {"name":"Tranquil Cove","count":1},{"name":"Wind-Scarred Crag","count":1},
            {"name":"Plains","count":29},{"name":"Ajani's Pridemate","count":1},
            {"name":"Marauding Blight-Priest","count":1},{"name":"Sanguine Syphoner","count":1}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    assert_eq!(&ids[..8], &[243, 244, 245, 246, 247, 248, 249, 250]);
    assert_eq!(&ids[37..], &[251, 252, 253]);
}

#[test]
fn gainlands_enter_tapped_gain_one_life_and_tap_for_either_color() {
    for (name, colors) in GAINLANDS {
        for color in colors {
            let mut state = ready(Step::Main1);
            let land = put(&mut state, PlayerId::P0, name, Zone::Hand);
            let mut surface = surface();
            next(&mut surface, &mut state);
            apply(&mut surface, &mut state, Action::PlayLand(land));
            assert!(state.objects.get(land).tapped, "{name}");
            assert_eq!(life(&state), (20, 20), "{name}");
            assert!(
                engine::step(&mut state, Action::ActivateManaAbilityChoice(land, color)).is_err()
            );
            drain(&mut surface, &mut state);
            assert_eq!(life(&state), (21, 20), "{name}");
            state.objects.get_mut(land).tapped = false;
            next(&mut surface, &mut state);
            apply(
                &mut surface,
                &mut state,
                Action::ActivateManaAbilityChoice(land, color),
            );
            assert_eq!(state.players[0].mana_pool[color.pool_index()], 1, "{name}");
            assert_eq!(state.players[0].mana_pool.iter().sum::<u8>(), 1, "{name}");
            assert!(state.stack.is_empty());
        }
    }
}

#[test]
fn creatures_cast_for_their_exact_costs() {
    for (name, mana) in [
        ("Ajani's Pridemate", [1, 0, 0, 0, 0, 1]),
        ("Marauding Blight-Priest", [0, 0, 1, 0, 0, 2]),
        ("Sanguine Syphoner", [0, 0, 1, 0, 0, 1]),
    ] {
        let mut state = ready(Step::Main1);
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        let mut short = mana;
        short[5] -= 1;
        state.players[0].mana_pool = short;
        assert!(
            engine::step(&mut state, Action::CastSpell(creature)).is_err(),
            "{name}"
        );
        state.players[0].mana_pool = mana;
        let mut surface = surface();
        next(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::CastSpell(creature));
        next(&mut surface, &mut state);
        assert_eq!(state.players[0].mana_pool, [0; 6], "{name}");
        drain(&mut surface, &mut state);
        assert_eq!(
            state.objects.get(creature).zone,
            Zone::Battlefield,
            "{name}"
        );
        assert_eq!(life(&state), (20, 20), "{name}");
    }
}

#[test]
fn pridemate_gets_a_counter_for_each_separate_life_gain() {
    let mut state = ready(Step::Main1);
    let pridemate = put(
        &mut state,
        PlayerId::P0,
        "Ajani's Pridemate",
        Zone::Battlefield,
    );
    let mut surface = surface();
    enter(&mut state, PlayerId::P0, "Scoured Barrens");
    drain(&mut surface, &mut state);
    assert_eq!(life(&state), (21, 20));
    assert_eq!(state.objects.get(pridemate).counters.plus1_plus1, 1);
    enter(&mut state, PlayerId::P0, "Tranquil Cove");
    drain(&mut surface, &mut state);
    assert_eq!(life(&state), (22, 20));
    assert_eq!(state.objects.get(pridemate).counters.plus1_plus1, 2);
    assert_eq!(engine::effective_power(&state, pridemate), 4);
    assert_eq!(engine::effective_toughness(&state, pridemate), 4);
}

#[test]
fn opponents_life_gain_triggers_neither_pridemate_nor_blight_priest() {
    let mut state = ready(Step::Main1);
    let pridemate = put(
        &mut state,
        PlayerId::P0,
        "Ajani's Pridemate",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Marauding Blight-Priest",
        Zone::Battlefield,
    );
    let mut surface = surface();
    enter(&mut state, PlayerId::P1, "Jungle Hollow");
    drain(&mut surface, &mut state);
    assert_eq!(life(&state), (20, 21));
    assert_eq!(state.objects.get(pridemate).counters.plus1_plus1, 0);
}

#[test]
fn blight_priest_drains_once_per_life_gain_alongside_pridemate() {
    let mut state = ready(Step::Main1);
    let pridemate = put(
        &mut state,
        PlayerId::P0,
        "Ajani's Pridemate",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Marauding Blight-Priest",
        Zone::Battlefield,
    );
    let mut surface = surface();
    enter(&mut state, PlayerId::P0, "Dismal Backwater");
    drain(&mut surface, &mut state);
    assert_eq!(life(&state), (21, 19));
    assert_eq!(state.objects.get(pridemate).counters.plus1_plus1, 1);
}

#[test]
fn syphoner_attack_drains_one_and_its_life_gain_feeds_other_triggers() {
    let mut state = ready(Step::DeclareAttackers);
    let syphoner = put(
        &mut state,
        PlayerId::P0,
        "Sanguine Syphoner",
        Zone::Battlefield,
    );
    let pridemate = put(
        &mut state,
        PlayerId::P0,
        "Ajani's Pridemate",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Marauding Blight-Priest",
        Zone::Battlefield,
    );
    let mut surface = surface();
    assert!(matches!(
        next(&mut surface, &mut state),
        Decision::DeclareAttackers { .. }
    ));
    apply(
        &mut surface,
        &mut state,
        Action::DeclareAttackers(vec![syphoner]),
    );
    drain(&mut surface, &mut state);
    // Syphoner: opponent loses 1, controller gains 1. That gain triggers
    // Pridemate's counter and Blight-Priest's additional drain.
    assert_eq!(life(&state), (21, 18));
    assert_eq!(state.objects.get(pridemate).counters.plus1_plus1, 1);
    assert_eq!(state.step, Step::DeclareAttackers);
}

#[test]
fn syphoner_trigger_resolves_after_it_leaves_the_battlefield() {
    let mut state = ready(Step::DeclareAttackers);
    let syphoner = put(
        &mut state,
        PlayerId::P0,
        "Sanguine Syphoner",
        Zone::Battlefield,
    );
    let mut surface = surface();
    next(&mut surface, &mut state);
    apply(
        &mut surface,
        &mut state,
        Action::DeclareAttackers(vec![syphoner]),
    );
    next(&mut surface, &mut state);
    assert_eq!(state.stack.len(), 1);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(syphoner, Zone::Graveyard),
    );
    drain(&mut surface, &mut state);
    assert_eq!(life(&state), (21, 19));
}
