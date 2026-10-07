//! FDN keyword-only reference creatures: definitions, casting and combat.
#![cfg(feature = "limited-fdn-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::CommittedEvent;
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomDeckV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};

type ExpectedDefinition = (
    &'static str,
    u8,
    &'static [ManaColor],
    &'static [Subtype],
    (i16, i16),
    Keywords,
);

const NAMES: [&str; 7] = [
    "Aegis Turtle",
    "Brazen Scourge",
    "Quakestrider Ceratops",
    "Savannah Lions",
    "Serra Angel",
    "Swiftblade Vindicator",
    "Vampire Nighthawk",
];

fn ready(step: Step) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = step;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
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
        _ => panic!("test helper zone"),
    }
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn priority(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(Decision::CastSpellOrPass { .. }) => {}
        other => panic!("expected priority, got {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn pass_both(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    priority(surface, state);
    apply(surface, state, Action::Pass);
    priority(surface, state);
    apply(surface, state, Action::Pass);
}

/// Declares `attackers`, then the defending player's `blockers` for the first
/// attacker, returning the blockers that were legal for it.
fn attack(
    surface: &mut HarnessSurfaceV2,
    state: &mut GameState,
    attackers: Vec<ObjectId>,
    blockers: Vec<ObjectId>,
) -> Vec<ObjectId> {
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    assert!(matches!(
        surface.next_decision(state),
        SurfaceDecision::Decision(Decision::DeclareAttackers { .. })
    ));
    apply(surface, state, Action::DeclareAttackers(attackers));
    pass_both(surface, state);
    let legal = match surface.next_decision(state) {
        SurfaceDecision::DeclareBlockersForAttacker { legal_blockers, .. } => legal_blockers,
        other => panic!("blocker choice: {other:?}"),
    };
    surface
        .apply(state, SurfaceAction::DeclareBlockersForAttacker(blockers))
        .unwrap();
    legal
}

fn dealt(state: &GameState, source: ObjectId, target: Target) -> i32 {
    state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: actual,
                target: actual_target,
                amount,
            } if *actual == source && *actual_target == target => Some(*amount),
            _ => None,
        })
        .sum()
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    let expected: [ExpectedDefinition; 7] = [
        (
            "Aegis Turtle",
            1,
            &[ManaColor::U],
            &[Subtype::Turtle],
            (0, 5),
            Keywords::NONE,
        ),
        (
            "Brazen Scourge",
            3,
            &[ManaColor::R],
            &[Subtype::Gremlin],
            (3, 3),
            Keywords::HASTE,
        ),
        (
            "Quakestrider Ceratops",
            6,
            &[ManaColor::G],
            &[Subtype::Dinosaur],
            (12, 8),
            Keywords::NONE,
        ),
        (
            "Savannah Lions",
            1,
            &[ManaColor::W],
            &[Subtype::Cat],
            (2, 1),
            Keywords::NONE,
        ),
        (
            "Serra Angel",
            5,
            &[ManaColor::W],
            &[Subtype::Angel],
            (4, 4),
            Keywords(Keywords::FLYING.0 | Keywords::VIGILANCE.0),
        ),
        (
            "Swiftblade Vindicator",
            2,
            &[ManaColor::R, ManaColor::W],
            &[Subtype::Human, Subtype::Soldier],
            (1, 1),
            Keywords(Keywords::DOUBLE_STRIKE.0 | Keywords::VIGILANCE.0 | Keywords::TRAMPLE.0),
        ),
        (
            "Vampire Nighthawk",
            3,
            &[ManaColor::B],
            &[Subtype::Vampire, Subtype::Shaman],
            (2, 3),
            Keywords(Keywords::FLYING.0 | Keywords::DEATHTOUCH.0 | Keywords::LIFELINK.0),
        ),
    ];
    let first = card_id_by_name("Aegis Turtle").unwrap() as usize;
    assert_eq!(first, 236);
    for (offset, (name, mana_value, colors, subtypes, (power, toughness), keywords)) in
        expected.into_iter().enumerate()
    {
        let id = card_id_by_name(name).unwrap() as usize;
        assert_eq!(id, first + offset, "{name}");
        let def = &CARD_DEFS[id];
        assert_eq!(def.capability, CardCapability::Full, "{name}");
        assert!(!def.is_token, "{name}");
        assert_eq!(def.mana_value as u8, mana_value, "{name}");
        assert_eq!(def.colors, colors, "{name}");
        assert_eq!(def.subtypes, subtypes, "{name}");
        assert_eq!(
            (def.power, def.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        assert_eq!(def.keywords, keywords, "{name}");
    }
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Turtle));
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Gremlin));
    assert!(Subtype::CREATURE_TYPES.contains(&Subtype::Dinosaur));
}

#[test]
fn reference_deck_resolves_all_forty_copies_in_row_order() {
    let deck: CustomDeckV1 = serde_json::from_str(
        r#"{"cards":[{"name":"Plains","count":33},{"name":"Aegis Turtle","count":1},
            {"name":"Brazen Scourge","count":1},{"name":"Quakestrider Ceratops","count":1},
            {"name":"Savannah Lions","count":1},{"name":"Serra Angel","count":1},
            {"name":"Swiftblade Vindicator","count":1},{"name":"Vampire Nighthawk","count":1}]}"#,
    )
    .unwrap();
    let ids = deck.resolve().unwrap();
    assert_eq!(ids.len(), 40);
    assert_eq!(&ids[33..], &[236, 237, 238, 239, 240, 241, 242]);
}

#[test]
fn each_creature_casts_at_sorcery_speed_for_its_exact_cost() {
    // Pool order is W, U, B, R, G, colorless; generic is paid from colorless.
    for (name, mana) in [
        ("Aegis Turtle", [0, 1, 0, 0, 0, 0]),
        ("Brazen Scourge", [0, 0, 0, 2, 0, 1]),
        ("Quakestrider Ceratops", [0, 0, 0, 0, 3, 3]),
        ("Savannah Lions", [1, 0, 0, 0, 0, 0]),
        ("Serra Angel", [2, 0, 0, 0, 0, 3]),
        ("Swiftblade Vindicator", [1, 0, 0, 1, 0, 0]),
        ("Vampire Nighthawk", [0, 0, 2, 0, 0, 1]),
    ] {
        let mut state = ready(Step::Main1);
        let creature = put(&mut state, PlayerId::P0, name, Zone::Hand);
        assert!(
            engine::step(&mut state, Action::CastSpell(creature)).is_err(),
            "{name}"
        );
        let mut short = mana;
        let last = short.iter().rposition(|&amount| amount > 0).unwrap();
        short[last] -= 1;
        state.players[0].mana_pool = short;
        assert!(
            engine::step(&mut state, Action::CastSpell(creature)).is_err(),
            "{name}"
        );
        state.players[0].mana_pool = mana;
        let mut surface = surface();
        priority(&mut surface, &mut state);
        apply(&mut surface, &mut state, Action::CastSpell(creature));
        priority(&mut surface, &mut state);
        assert_eq!(state.players[0].mana_pool, [0; 6], "{name}");
        pass_both(&mut surface, &mut state);
        priority(&mut surface, &mut state);
        assert_eq!(
            state.objects.get(creature).zone,
            Zone::Battlefield,
            "{name}"
        );

        let mut combat = ready(Step::Main2);
        let opposing = put(&mut combat, PlayerId::P0, name, Zone::Hand);
        combat.players[0].mana_pool = mana;
        combat.priority_player = PlayerId::P1;
        assert!(
            engine::step(&mut combat, Action::CastSpell(opposing)).is_err(),
            "{name}"
        );
    }
}

#[test]
fn only_haste_lets_a_summoning_sick_creature_attack() {
    let mut state = ready(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = false;
    let scourge = put(
        &mut state,
        PlayerId::P0,
        "Brazen Scourge",
        Zone::Battlefield,
    );
    let lions = put(
        &mut state,
        PlayerId::P0,
        "Savannah Lions",
        Zone::Battlefield,
    );
    state.objects.get_mut(scourge).summoning_sick = true;
    state.objects.get_mut(lions).summoning_sick = true;
    let mut surface = surface();
    match surface.next_decision(&mut state) {
        SurfaceDecision::Decision(Decision::DeclareAttackers { eligible, .. }) => {
            assert!(eligible.contains(&scourge));
            assert!(!eligible.contains(&lions));
        }
        other => panic!("attack choice: {other:?}"),
    }
}

#[test]
fn flyers_evade_ground_blockers_and_vigilance_keeps_attackers_untapped() {
    let mut state = ready(Step::DeclareAttackers);
    let angel = put(&mut state, PlayerId::P0, "Serra Angel", Zone::Battlefield);
    let turtle = put(&mut state, PlayerId::P1, "Aegis Turtle", Zone::Battlefield);
    let nighthawk = put(
        &mut state,
        PlayerId::P1,
        "Vampire Nighthawk",
        Zone::Battlefield,
    );
    let mut surface = surface();
    let legal = attack(&mut surface, &mut state, vec![angel], vec![]);
    assert!(legal.contains(&nighthawk));
    assert!(!legal.contains(&turtle));
    assert!(!state.objects.get(angel).tapped);
    pass_both(&mut surface, &mut state);
    priority(&mut surface, &mut state);
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(state.players[1].life, 16);
}

#[test]
fn nighthawk_deathtouch_kills_the_ceratops_and_lifelink_gains_its_damage() {
    let mut state = ready(Step::DeclareAttackers);
    let ceratops = put(
        &mut state,
        PlayerId::P0,
        "Quakestrider Ceratops",
        Zone::Battlefield,
    );
    let nighthawk = put(
        &mut state,
        PlayerId::P1,
        "Vampire Nighthawk",
        Zone::Battlefield,
    );
    let mut surface = surface();
    let legal = attack(&mut surface, &mut state, vec![ceratops], vec![nighthawk]);
    assert!(legal.contains(&nighthawk));
    assert!(state.objects.get(ceratops).tapped);
    pass_both(&mut surface, &mut state);
    priority(&mut surface, &mut state);
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(state.objects.get(ceratops).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(nighthawk).zone, Zone::Graveyard);
    assert_eq!(dealt(&state, nighthawk, Target::Object(ceratops)), 2);
    assert_eq!((state.players[0].life, state.players[1].life), (20, 22));
}

#[test]
fn aegis_turtle_survives_blocking_a_three_power_attacker() {
    let mut state = ready(Step::DeclareAttackers);
    let scourge = put(
        &mut state,
        PlayerId::P0,
        "Brazen Scourge",
        Zone::Battlefield,
    );
    let turtle = put(&mut state, PlayerId::P1, "Aegis Turtle", Zone::Battlefield);
    let mut surface = surface();
    attack(&mut surface, &mut state, vec![scourge], vec![turtle]);
    pass_both(&mut surface, &mut state);
    priority(&mut surface, &mut state);
    assert_eq!(state.objects.get(turtle).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(turtle).damage, 3);
    assert_eq!(state.objects.get(scourge).damage, 0);
    assert_eq!(state.players[1].life, 20);
}

#[test]
fn vindicator_first_strike_kills_its_blocker_then_tramples_over_in_the_normal_wave() {
    let mut state = ready(Step::DeclareAttackers);
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    enable_foundations_combat_v1(&mut state).unwrap();
    let vindicator = put(
        &mut state,
        PlayerId::P0,
        "Swiftblade Vindicator",
        Zone::Battlefield,
    );
    let lions = put(
        &mut state,
        PlayerId::P1,
        "Savannah Lions",
        Zone::Battlefield,
    );
    let mut surface = surface();
    attack(&mut surface, &mut state, vec![vindicator], vec![lions]);
    assert!(!state.objects.get(vindicator).tapped);
    pass_both(&mut surface, &mut state);
    // Lethal damage to the only blocker uses all of the first-strike power,
    // so the first wave needs no assignment choice.
    priority(&mut surface, &mut state);
    assert_eq!(state.step, Step::CombatDamage);
    assert_eq!(state.objects.get(lions).zone, Zone::Graveyard);
    assert_eq!(dealt(&state, vindicator, Target::Object(lions)), 1);
    assert_eq!(state.players[1].life, 20);
    pass_both(&mut surface, &mut state);
    priority(&mut surface, &mut state);
    assert_eq!(dealt(&state, lions, Target::Object(vindicator)), 0);
    assert_eq!(state.objects.get(vindicator).damage, 0);
    assert_eq!(dealt(&state, vindicator, Target::Player(PlayerId::P1)), 1);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn unblocked_vindicator_deals_damage_in_both_waves() {
    let mut state = ready(Step::DeclareAttackers);
    let vindicator = put(
        &mut state,
        PlayerId::P0,
        "Swiftblade Vindicator",
        Zone::Battlefield,
    );
    let mut surface = surface();
    state.engine.combat.attackers_declared = false;
    state.engine.combat.blockers_declared = false;
    assert!(matches!(
        surface.next_decision(&mut state),
        SurfaceDecision::Decision(Decision::DeclareAttackers { .. })
    ));
    apply(
        &mut surface,
        &mut state,
        Action::DeclareAttackers(vec![vindicator]),
    );
    for _ in 0..8 {
        if state.players[1].life == 18 {
            break;
        }
        match surface.next_decision(&mut state) {
            SurfaceDecision::Decision(Decision::CastSpellOrPass { .. }) => {
                apply(&mut surface, &mut state, Action::Pass)
            }
            SurfaceDecision::DeclareBlockersForAttacker { .. } => surface
                .apply(
                    &mut state,
                    SurfaceAction::DeclareBlockersForAttacker(vec![]),
                )
                .unwrap(),
            other => panic!("unexpected decision {other:?}"),
        }
    }
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn reference_names_are_all_registry_full() {
    for name in NAMES {
        let id = card_id_by_name(name).unwrap() as usize;
        assert!(CARD_DEFS[id].has_full_support(), "{name}");
    }
}
