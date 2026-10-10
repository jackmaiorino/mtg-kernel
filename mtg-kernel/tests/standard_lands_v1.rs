//! MageZero Standard lands batch 1 (`docs/design/standard_lands_v1.md`):
//! definitions, conditional entry, conditional and restricted mana, land
//! animation, channel, cycling, surveil entry, and toxic.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

const PAINLANDS: [(&str, [ManaColor; 2]); 10] = [
    ("Adarkar Wastes", [ManaColor::W, ManaColor::U]),
    ("Battlefield Forge", [ManaColor::R, ManaColor::W]),
    ("Brushland", [ManaColor::G, ManaColor::W]),
    ("Caves of Koilos", [ManaColor::W, ManaColor::B]),
    ("Karplusan Forest", [ManaColor::R, ManaColor::G]),
    ("Llanowar Wastes", [ManaColor::B, ManaColor::G]),
    ("Shivan Reef", [ManaColor::U, ManaColor::R]),
    ("Sulfurous Springs", [ManaColor::B, ManaColor::R]),
    ("Underground River", [ManaColor::U, ManaColor::B]),
    ("Yavimaya Coast", [ManaColor::G, ManaColor::U]),
];

/// (verge, unconditional color, conditional color, a basic that enables it).
const VERGES: [(&str, ManaColor, ManaColor, &str); 6] = [
    ("Floodfarm Verge", ManaColor::W, ManaColor::U, "Island"),
    ("Gloomlake Verge", ManaColor::U, ManaColor::B, "Swamp"),
    ("Hushwood Verge", ManaColor::G, ManaColor::W, "Plains"),
    ("Riverpyre Verge", ManaColor::R, ManaColor::U, "Mountain"),
    ("Thornspire Verge", ManaColor::R, ManaColor::G, "Forest"),
    ("Wastewood Verge", ManaColor::G, ManaColor::B, "Swamp"),
];

const FASTLANDS: [&str; 9] = [
    "Blackcleave Cliffs",
    "Blooming Marsh",
    "Concealed Courtyard",
    "Copperline Gorge",
    "Darkslick Shores",
    "Inspiring Vantage",
    "Razorverge Thicket",
    "Seachrome Coast",
    "Spirebluff Canal",
];

const SLOWLANDS: [&str; 5] = [
    "Deserted Beach",
    "Dreamroot Cascade",
    "Haunted Ridge",
    "Overgrown Farmland",
    "Rockfall Vale",
];

const SURVEIL_LANDS: [(&str, [Subtype; 2]); 3] = [
    ("Elegant Parlor", [Subtype::Mountain, Subtype::Plains]),
    ("Lush Portico", [Subtype::Forest, Subtype::Plains]),
    ("Underground Mortuary", [Subtype::Swamp, Subtype::Forest]),
];

const TRIOMES: [(&str, [Subtype; 3]); 3] = [
    (
        "Jetmir's Garden",
        [Subtype::Mountain, Subtype::Forest, Subtype::Plains],
    ),
    (
        "Spara's Headquarters",
        [Subtype::Forest, Subtype::Plains, Subtype::Island],
    ),
    (
        "Ziatora's Proving Ground",
        [Subtype::Swamp, Subtype::Mountain, Subtype::Forest],
    ),
];

const MONO_LANDS: [&str; 5] = [
    "Mishra's Foundry",
    "Eiganjo, Seat of the Empire",
    "Mirrex",
    "Rockface Village",
    "Starting Town",
];

fn ready() -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state =
        GameState::new_from_libraries(&[plains; 30], &[plains; 30], |_| "Plains".into(), 0x4c4e44);
    state.step = Step::Main1;
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap_or_else(|| panic!("{name}"));
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
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority (and answers combat and trigger-order questions with
/// nothing) until `stop` accepts a decision, which is returned.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState, &Decision) -> bool) -> Decision {
    for _ in 0..200 {
        let decision = next(state);
        if stop(state, &decision) {
            return decision;
        }
        let action = match &decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
            Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
            other => panic!("unexpected decision: {other:?}"),
        };
        engine::step(state, action).unwrap();
    }
    panic!("pass_until did not stop");
}

/// Passes until priority returns with an empty stack and no pending triggers.
fn resolve_stack(state: &mut GameState) {
    pass_until(state, |state, decision| {
        matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
    });
}

fn play(state: &mut GameState, land: ObjectId) {
    next(state);
    engine::step(state, Action::PlayLand(land)).unwrap();
}

/// Moves `id` onto the battlefield as if put there by an effect.
fn enter(state: &mut GameState, id: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(id, Zone::Battlefield));
}

fn castable(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn activatable(state: &mut GameState) -> Vec<(ObjectId, u8)> {
    match next(state) {
        Decision::CastSpellOrPass {
            activatable_abilities,
            ..
        } => activatable_abilities,
        other => panic!("expected priority, got {other:?}"),
    }
}

fn pool(state: &GameState, player: PlayerId) -> [u8; 6] {
    state.players[player.index()].mana_pool
}

fn mana(color: ManaColor) -> [u8; 6] {
    let mut pool = [0; 6];
    pool[color.pool_index()] = 1;
    pool
}

fn def(name: &str) -> &'static mtg_kernel::card_def::CardDef {
    &CARD_DEFS[card_id_by_name(name).unwrap() as usize]
}

#[test]
fn every_land_in_the_batch_has_the_declared_support_and_characteristics() {
    let names = PAINLANDS
        .iter()
        .map(|(name, _)| *name)
        .chain(VERGES.iter().map(|(name, ..)| *name))
        .chain(FASTLANDS)
        .chain(SLOWLANDS)
        .chain(SURVEIL_LANDS.iter().map(|(name, _)| *name))
        .chain(TRIOMES.iter().map(|(name, _)| *name))
        .chain(MONO_LANDS)
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 41);
    for name in names {
        let def = def(name);
        let expected = if matches!(name, "Mirrex" | "Rockface Village") {
            CardCapability::Partial
        } else {
            CardCapability::Full
        };
        assert_eq!(def.capability, expected, "{name}");
        assert!(def.is_land, "{name}");
        assert_eq!(def.types, &[CardType::Land], "{name}");
        assert!(def.colors.is_empty(), "{name}");
    }
    for (name, subtypes) in SURVEIL_LANDS {
        assert_eq!(def(name).subtypes, &subtypes, "{name}");
        assert!(def(name).enters_battlefield_tapped, "{name}");
    }
    for (name, subtypes) in TRIOMES {
        assert_eq!(def(name).subtypes, &subtypes, "{name}");
        assert!(def(name).enters_battlefield_tapped, "{name}");
    }
    assert_eq!(
        def("Eiganjo, Seat of the Empire").supertypes,
        &[Supertype::Legendary]
    );
    assert_eq!(def("Mirrex").subtypes, &[Subtype::Sphere]);
    assert_eq!(def("Starting Town").subtypes, &[Subtype::Town]);

    let mite = def("Phyrexian Mite Token");
    assert!(mite.is_token);
    assert_eq!(mite.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(mite.subtypes, &[Subtype::Phyrexian, Subtype::Mite]);
    assert_eq!((mite.power, mite.toughness), (Some(1), Some(1)));
    assert!(mite.colors.is_empty());
    assert!(mite.keywords.has(Keywords::TOXIC_1));
}

#[test]
fn fastlands_enter_untapped_only_with_two_or_fewer_other_lands() {
    for name in FASTLANDS {
        for (others, tapped) in [(0, false), (2, false), (3, true)] {
            let mut state = ready();
            for _ in 0..others {
                put(&mut state, PlayerId::P0, "Plains", Zone::Battlefield);
            }
            // An opponent's lands never count.
            put(&mut state, PlayerId::P1, "Plains", Zone::Battlefield);
            put(&mut state, PlayerId::P1, "Plains", Zone::Battlefield);
            put(&mut state, PlayerId::P1, "Plains", Zone::Battlefield);
            let land = put(&mut state, PlayerId::P0, name, Zone::Hand);
            play(&mut state, land);
            assert_eq!(
                state.objects.get(land).tapped,
                tapped,
                "{name} with {others}"
            );
        }
    }
}

#[test]
fn slowlands_enter_untapped_only_with_two_or_more_other_lands() {
    for name in SLOWLANDS {
        for (others, tapped) in [(0, true), (1, true), (2, false), (4, false)] {
            let mut state = ready();
            for _ in 0..others {
                put(&mut state, PlayerId::P0, "Plains", Zone::Battlefield);
            }
            let land = put(&mut state, PlayerId::P0, name, Zone::Hand);
            play(&mut state, land);
            assert_eq!(
                state.objects.get(land).tapped,
                tapped,
                "{name} with {others}"
            );
        }
    }
}

#[test]
fn starting_town_enters_untapped_on_its_controllers_first_three_turns() {
    for (turn, tapped) in [(1, false), (3, false), (4, true)] {
        let mut state = ready();
        state.turn = turn;
        let town = put(&mut state, PlayerId::P0, "Starting Town", Zone::Hand);
        play(&mut state, town);
        assert_eq!(state.objects.get(town).tapped, tapped, "turn {turn}");
    }
    // On the opponent's turn it enters tapped.
    let mut state = ready();
    state.active_player = PlayerId::P1;
    let town = put(&mut state, PlayerId::P0, "Starting Town", Zone::Hand);
    enter(&mut state, town);
    assert!(state.objects.get(town).tapped);
}

#[test]
fn starting_town_pays_one_life_for_any_color() {
    let mut state = ready();
    let town = put(&mut state, PlayerId::P0, "Starting Town", Zone::Battlefield);
    next(&mut state);
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(town, ManaColor::C),
    )
    .unwrap();
    assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::C));
    assert_eq!(state.players[0].life, 20);

    for color in [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ] {
        let mut state = ready();
        let town = put(&mut state, PlayerId::P0, "Starting Town", Zone::Battlefield);
        next(&mut state);
        engine::step(&mut state, Action::ActivateManaAbilityChoice(town, color)).unwrap();
        assert_eq!(pool(&state, PlayerId::P0), mana(color));
        assert_eq!(state.players[0].life, 19);
        assert!(state.objects.get(town).tapped);
    }

    // 119.4: paying 1 life needs at least 1 life. Life loss is not damage.
    let mut state = ready();
    state.players[0].life = 0;
    let town = put(&mut state, PlayerId::P0, "Starting Town", Zone::Battlefield);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(town, ManaColor::W)
    )
    .is_err());
}

#[test]
fn painlands_tap_for_colorless_freely_and_deal_one_damage_for_a_color() {
    for (name, colors) in PAINLANDS {
        let mut state = ready();
        let land = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        next(&mut state);
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(land, ManaColor::C),
        )
        .unwrap();
        assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::C), "{name}");
        assert_eq!(state.players[0].life, 20, "{name}");
        for color in colors {
            let mut state = ready();
            let land = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
            next(&mut state);
            engine::step(&mut state, Action::ActivateManaAbilityChoice(land, color)).unwrap();
            assert_eq!(pool(&state, PlayerId::P0), mana(color), "{name}");
            assert_eq!(state.players[0].life, 19, "{name}");
        }
        let other = [
            ManaColor::W,
            ManaColor::U,
            ManaColor::B,
            ManaColor::R,
            ManaColor::G,
        ]
        .into_iter()
        .find(|color| !colors.contains(color))
        .unwrap();
        let mut state = ready();
        let land = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        assert!(
            engine::step(&mut state, Action::ActivateManaAbilityChoice(land, other)).is_err(),
            "{name}"
        );
    }
}

#[test]
fn verges_add_their_second_color_only_with_a_matching_basic_type() {
    for (name, primary, conditional, enabler) in VERGES {
        let mut state = ready();
        let verge = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        next(&mut state);
        assert!(
            engine::step(
                &mut state,
                Action::ActivateManaAbilityChoice(verge, conditional)
            )
            .is_err(),
            "{name} without {enabler}"
        );
        engine::step(&mut state, Action::ActivateManaAbility(verge)).unwrap();
        assert_eq!(pool(&state, PlayerId::P0), mana(primary), "{name}");

        let mut state = ready();
        let verge = put(&mut state, PlayerId::P0, name, Zone::Battlefield);
        // The opponent's basic does not enable it.
        put(&mut state, PlayerId::P1, enabler, Zone::Battlefield);
        assert!(engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(verge, conditional)
        )
        .is_err());
        put(&mut state, PlayerId::P0, enabler, Zone::Battlefield);
        next(&mut state);
        engine::step(
            &mut state,
            Action::ActivateManaAbilityChoice(verge, conditional),
        )
        .unwrap();
        assert_eq!(pool(&state, PlayerId::P0), mana(conditional), "{name}");
        assert_eq!(state.players[0].life, 20, "{name}");
    }
}

#[test]
fn mirrex_adds_any_color_only_the_turn_it_entered_and_makes_mites() {
    let mut state = ready();
    let mirrex = put(&mut state, PlayerId::P0, "Mirrex", Zone::Hand);
    play(&mut state, mirrex);
    assert!(!state.objects.get(mirrex).tapped);
    next(&mut state);
    engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(mirrex, ManaColor::G),
    )
    .unwrap();
    assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::G));

    // A Mirrex that has been around since an earlier turn taps only for {C}.
    let mut state = ready();
    let mirrex = put(&mut state, PlayerId::P0, "Mirrex", Zone::Battlefield);
    assert!(!state.objects.get(mirrex).v4.entered_battlefield_this_turn);
    next(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(mirrex, ManaColor::G)
    )
    .is_err());
    engine::step(&mut state, Action::ActivateManaAbility(mirrex)).unwrap();
    assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::C));

    // {3}, {T}: create a 1/1 Phyrexian Mite.
    let mut state = ready();
    let mirrex = put(&mut state, PlayerId::P0, "Mirrex", Zone::Battlefield);
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 2;
    assert!(!activatable(&mut state).contains(&(mirrex, 0)));
    state.players[0].mana_pool[ManaColor::W.pool_index()] = 3;
    assert!(activatable(&mut state).contains(&(mirrex, 0)));
    engine::step(&mut state, Action::ActivateAbility(mirrex, 0)).unwrap();
    next(&mut state);
    assert!(state.objects.get(mirrex).tapped);
    assert_eq!(pool(&state, PlayerId::P0), [0; 6]);
    resolve_stack(&mut state);
    let mites = state.players[0]
        .battlefield
        .iter()
        .filter(|&&id| {
            state.objects.get(id).card_def == card_id_by_name("Phyrexian Mite Token").unwrap()
        })
        .count();
    assert_eq!(mites, 1);
}

#[test]
fn rockface_village_red_pays_creature_spells_only() {
    let mut state = ready();
    let village = put(
        &mut state,
        PlayerId::P0,
        "Rockface Village",
        Zone::Battlefield,
    );
    let epicure = put(&mut state, PlayerId::P0, "Voldaren Epicure", Zone::Hand);
    let burst = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let spells = castable(&mut state);
    assert!(spells.contains(&epicure));
    assert!(!spells.contains(&burst));
    // The restricted red is never floated.
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(village, ManaColor::R)
    )
    .is_err());
    engine::step(&mut state, Action::CastSpell(epicure)).unwrap();
    next(&mut state);
    assert_eq!(state.objects.get(epicure).zone, Zone::Stack);
    assert!(state.objects.get(village).tapped);
    assert_eq!(pool(&state, PlayerId::P0), [0; 6]);

    // Its {C} still pays anything generic.
    let mut state = ready();
    let village = put(
        &mut state,
        PlayerId::P0,
        "Rockface Village",
        Zone::Battlefield,
    );
    next(&mut state);
    engine::step(&mut state, Action::ActivateManaAbility(village)).unwrap();
    assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::C));
}

/// Activates Mishra's Foundry's `{2}` animation with floating mana and lets
/// it resolve.
fn animate(state: &mut GameState, foundry: ObjectId) {
    state.players[state.objects.get(foundry).controller.index()].mana_pool
        [ManaColor::C.pool_index()] += 2;
    next(state);
    engine::step(state, Action::ActivateAbility(foundry, 0)).unwrap();
    resolve_stack(state);
}

#[test]
fn mishras_foundry_animates_attacks_and_reverts_at_cleanup() {
    let mut state = ready();
    let foundry = put(
        &mut state,
        PlayerId::P0,
        "Mishra's Foundry",
        Zone::Battlefield,
    );
    let pumper = put(
        &mut state,
        PlayerId::P0,
        "Mishra's Foundry",
        Zone::Battlefield,
    );
    assert!(!engine::object_has_type(
        &state,
        foundry,
        CardType::Creature
    ));
    animate(&mut state, foundry);
    for card_type in [CardType::Artifact, CardType::Creature, CardType::Land] {
        assert!(engine::object_has_type(&state, foundry, card_type));
    }
    assert!(engine::has_effective_subtype(
        &state,
        foundry,
        Subtype::AssemblyWorker
    ));
    assert_eq!(engine::effective_power(&state, foundry), 2);
    assert_eq!(engine::effective_toughness(&state, foundry), 2);
    assert!(!engine::object_has_type(&state, pumper, CardType::Creature));

    let Decision::DeclareAttackers { eligible, .. } = pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    }) else {
        unreachable!()
    };
    assert_eq!(eligible, vec![foundry]);
    engine::step(&mut state, Action::DeclareAttackers(vec![foundry])).unwrap();

    // The other Foundry's {1}, {T}: target attacking Assembly-Worker +2/+2.
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    let decision = pass_until(&mut state, |state, d| {
        matches!(
            d,
            Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }
        ) && state.step == Step::DeclareAttackers
    });
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = decision
    else {
        unreachable!()
    };
    assert!(activatable_abilities.contains(&(pumper, 1)));
    assert!(
        !activatable_abilities.contains(&(foundry, 1)),
        "attacking Foundry is tapped"
    );
    engine::step(&mut state, Action::ActivateAbility(pumper, 1)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("expected targets")
    };
    assert_eq!(legal_targets, vec![Target::Object(foundry)]);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(foundry))).unwrap();
    resolve_stack(&mut state);
    assert_eq!(engine::effective_power(&state, foundry), 4);

    pass_until(&mut state, |state, _| state.step == Step::Main2);
    assert_eq!(state.players[1].life, 16);
    assert!(engine::object_has_type(&state, foundry, CardType::Creature));

    pass_until(&mut state, |state, _| state.active_player == PlayerId::P1);
    for card_type in [CardType::Artifact, CardType::Creature] {
        assert!(!engine::object_has_type(&state, foundry, card_type));
    }
    assert!(engine::object_has_type(&state, foundry, CardType::Land));
    assert_eq!(state.objects.get(foundry).zone, Zone::Battlefield);
}

#[test]
fn an_animated_foundry_has_summoning_sickness_and_dies_to_lethal_damage() {
    let mut state = ready();
    let foundry = put(&mut state, PlayerId::P0, "Mishra's Foundry", Zone::Hand);
    play(&mut state, foundry);
    animate(&mut state, foundry);
    let Decision::DeclareAttackers { eligible, .. } = pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    }) else {
        unreachable!()
    };
    assert!(eligible.is_empty());

    let mut state = ready();
    let foundry = put(
        &mut state,
        PlayerId::P0,
        "Mishra's Foundry",
        Zone::Battlefield,
    );
    animate(&mut state, foundry);
    state.objects.get_mut(foundry).damage = 2;
    trigger::sba_fixed_point(&mut state);
    assert_eq!(state.objects.get(foundry).zone, Zone::Graveyard);

    // Damage on an unanimated land is not lethal.
    let mut state = ready();
    let foundry = put(
        &mut state,
        PlayerId::P0,
        "Mishra's Foundry",
        Zone::Battlefield,
    );
    state.objects.get_mut(foundry).damage = 2;
    trigger::sba_fixed_point(&mut state);
    assert_eq!(state.objects.get(foundry).zone, Zone::Battlefield);
}

#[test]
fn eiganjo_channels_four_damage_to_an_attacking_creature() {
    let mut state = ready();
    let eiganjo = put(
        &mut state,
        PlayerId::P0,
        "Eiganjo, Seat of the Empire",
        Zone::Hand,
    );
    let elves = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    // Outside combat there is no legal target.
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 2];
    assert!(!activatable(&mut state).contains(&(eiganjo, 0)));
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(vec![elves])).unwrap();
    pass_until(&mut state, |state, d| {
        matches!(
            d,
            Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }
        ) && state.step == Step::DeclareAttackers
    });

    // Without a legendary creature the channel costs the full {2}{W}.
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 1];
    assert!(!activatable(&mut state).contains(&(eiganjo, 0)));
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 2];
    assert!(activatable(&mut state).contains(&(eiganjo, 0)));
    engine::step(&mut state, Action::ActivateAbility(eiganjo, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("expected targets")
    };
    assert_eq!(legal_targets, vec![Target::Object(elves)]);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(elves))).unwrap();
    next(&mut state);
    assert_eq!(state.objects.get(eiganjo).zone, Zone::Graveyard);
    resolve_stack(&mut state);
    assert_eq!(pool(&state, PlayerId::P0), [0; 6]);
    assert_eq!(state.objects.get(elves).zone, Zone::Graveyard);
}

#[test]
fn eiganjos_channel_costs_one_less_per_legendary_creature() {
    let mut state = ready();
    let eiganjo = put(
        &mut state,
        PlayerId::P0,
        "Eiganjo, Seat of the Empire",
        Zone::Hand,
    );
    let elves = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    // An opponent's legendary creature never counts.
    put(
        &mut state,
        PlayerId::P1,
        "Adeline, Resplendent Cathar",
        Zone::Battlefield,
    );
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(vec![elves])).unwrap();
    pass_until(&mut state, |state, d| {
        matches!(
            d,
            Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }
        ) && state.step == Step::DeclareAttackers
    });
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 0];
    assert!(!activatable(&mut state).contains(&(eiganjo, 0)));
    state.players[0].mana_pool = [1, 0, 0, 0, 0, 1];
    assert!(activatable(&mut state).contains(&(eiganjo, 0)));
    engine::step(&mut state, Action::ActivateAbility(eiganjo, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("expected targets")
    };
    assert!(legal_targets.contains(&Target::Object(elves)));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(elves))).unwrap();
    next(&mut state);
    assert_eq!(state.objects.get(eiganjo).zone, Zone::Graveyard);
    assert_eq!(pool(&state, PlayerId::P0), [0; 6], "paid {{1}}{{W}}");
}

#[test]
fn rockface_village_gives_a_mouse_plus_one_and_haste_at_sorcery_speed() {
    let mut state = ready();
    let village = put(
        &mut state,
        PlayerId::P0,
        "Rockface Village",
        Zone::Battlefield,
    );
    let mouse = put(
        &mut state,
        PlayerId::P0,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    let elves = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(mouse).summoning_sick = true;
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    assert!(activatable(&mut state).contains(&(village, 0)));
    engine::step(&mut state, Action::ActivateAbility(village, 0)).unwrap();
    let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
        panic!("expected targets")
    };
    assert_eq!(
        legal_targets,
        vec![Target::Object(mouse)],
        "not the Elf {elves:?}"
    );
    engine::step(&mut state, Action::ChooseTarget(Target::Object(mouse))).unwrap();
    resolve_stack(&mut state);
    assert_eq!(engine::effective_power(&state, mouse), 2);
    assert_eq!(engine::effective_toughness(&state, mouse), 2);
    assert!(engine::has_effective_keyword(
        &state,
        mouse,
        Keywords::HASTE
    ));
    let Decision::DeclareAttackers { eligible, .. } = pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    }) else {
        unreachable!()
    };
    assert!(eligible.contains(&mouse));

    // Not at instant speed.
    let mut state = ready();
    let village = put(
        &mut state,
        PlayerId::P0,
        "Rockface Village",
        Zone::Battlefield,
    );
    put(
        &mut state,
        PlayerId::P0,
        "Manifold Mouse",
        Zone::Battlefield,
    );
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap();
    state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
    let decision = pass_until(&mut state, |_, d| {
        matches!(
            d,
            Decision::CastSpellOrPass {
                player: PlayerId::P0,
                ..
            }
        )
    });
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = decision
    else {
        unreachable!()
    };
    assert!(!activatable_abilities.contains(&(village, 0)));
}

#[test]
fn eiganjo_taps_for_white() {
    let mut state = ready();
    let eiganjo = put(
        &mut state,
        PlayerId::P0,
        "Eiganjo, Seat of the Empire",
        Zone::Battlefield,
    );
    next(&mut state);
    engine::step(&mut state, Action::ActivateManaAbility(eiganjo)).unwrap();
    assert_eq!(pool(&state, PlayerId::P0), mana(ManaColor::W));
}

#[test]
fn triomes_enter_tapped_and_cycle_for_three() {
    for (name, _) in TRIOMES {
        let mut state = ready();
        let triome = put(&mut state, PlayerId::P0, name, Zone::Hand);
        play(&mut state, triome);
        assert!(state.objects.get(triome).tapped, "{name}");

        let mut state = ready();
        let triome = put(&mut state, PlayerId::P0, name, Zone::Hand);
        state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
        assert!(!activatable(&mut state).contains(&(triome, 0)), "{name}");
        state.players[0].mana_pool[ManaColor::C.pool_index()] = 3;
        assert!(activatable(&mut state).contains(&(triome, 0)), "{name}");
        let hand = state.players[0].hand.len();
        engine::step(&mut state, Action::ActivateAbility(triome, 0)).unwrap();
        next(&mut state);
        assert_eq!(state.objects.get(triome).zone, Zone::Graveyard, "{name}");
        resolve_stack(&mut state);
        assert_eq!(
            state.players[0].hand.len(),
            hand,
            "{name}: discarded one, drew one"
        );
    }
}

#[test]
fn surveil_lands_enter_tapped_and_surveil_one() {
    for (name, _) in SURVEIL_LANDS {
        let mut state = ready();
        let top = state.players[0].library[0];
        let land = put(&mut state, PlayerId::P0, name, Zone::Hand);
        play(&mut state, land);
        assert!(state.objects.get(land).tapped, "{name}");
        let decision = pass_until(&mut state, |_, d| {
            matches!(d, Decision::ChooseEffectOption { .. })
        });
        let Decision::ChooseEffectOption {
            option_count,
            source,
            ..
        } = decision
        else {
            unreachable!()
        };
        assert_eq!((option_count, source), (2, land), "{name}");
        engine::step(&mut state, Action::ChooseEffectOption(1)).unwrap();
        resolve_stack(&mut state);
        assert_eq!(state.players[0].graveyard, vec![top], "{name}");
    }
}

#[test]
fn phyrexian_mites_cant_block_and_toxic_poisons_to_a_loss() {
    // A Mite can't block.
    let mut state = ready();
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    let elves = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let mite = put(
        &mut state,
        PlayerId::P0,
        "Phyrexian Mite Token",
        Zone::Battlefield,
    );
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(vec![elves])).unwrap();
    let decision = pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareBlockers { .. })
    });
    if let Decision::DeclareBlockers { legal_blockers, .. } = decision {
        assert!(legal_blockers
            .iter()
            .all(|(_, blockers)| !blockers.contains(&mite)));
    }
    assert!(engine::step(&mut state, Action::DeclareBlockers(vec![(mite, elves)])).is_err());

    // Combat damage from a Mite gives one poison counter as well.
    let mut state = ready();
    let mite = put(
        &mut state,
        PlayerId::P0,
        "Phyrexian Mite Token",
        Zone::Battlefield,
    );
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(vec![mite])).unwrap();
    pass_until(&mut state, |state, _| state.step == Step::Main2);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.players[1].poison_counters.0, 1);
    assert_eq!(state.players[0].poison_counters.0, 0);

    // 704.5c: ten poison counters lose the game.
    let mut state = ready();
    state.players[1].poison_counters.0 = 9;
    let mite = put(
        &mut state,
        PlayerId::P0,
        "Phyrexian Mite Token",
        Zone::Battlefield,
    );
    pass_until(&mut state, |_, d| {
        matches!(d, Decision::DeclareAttackers { .. })
    });
    engine::step(&mut state, Action::DeclareAttackers(vec![mite])).unwrap();
    let decision = pass_until(&mut state, |_, d| matches!(d, Decision::GameOver { .. }));
    assert_eq!(
        decision,
        Decision::GameOver {
            winner: Some(PlayerId::P0)
        }
    );
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn players_without_poison_keep_their_state_bytes() {
    let state = ready();
    let json = serde_json::to_string(&state.players[0]).unwrap();
    assert!(!json.contains("poison"));
    let mut poisoned = state.clone();
    poisoned.players[1].poison_counters.0 = 1;
    assert!(serde_json::to_string(&poisoned.players[1])
        .unwrap()
        .contains("\"poison_counters\":1"));
}
