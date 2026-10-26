//! Eternal wave E2: native rules tests for the cards that complete Legacy
//! Dimir Tempo and Izzet Delver, plus the Orc Army token.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CastMode, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E2_CARDS: [&str; 27] = [
    "Underground Sea",
    "Volcanic Island",
    "Undercity Sewers",
    "Thundering Falls",
    "Bloodstained Mire",
    "Flooded Strand",
    "Marsh Flats",
    "Misty Rainforest",
    "Polluted Delta",
    "Scalding Tarn",
    "Wasteland",
    "Thoughtseize",
    "Fatal Push",
    "Unholy Heat",
    "Daze",
    "Force of Will",
    "Prismari Charm",
    "Brazen Borrower",
    "Mishra's Bauble",
    "Urza's Bauble",
    "Dragon's Rage Channeler",
    "Murktide Regent",
    "Moonshadow",
    "Nethergoyf",
    "Orcish Bowmasters",
    "Orc Army Token",
    "Tamiyo, Inquisitive Student",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[card_id_by_name(name).unwrap_or_else(|| panic!("{name}")) as usize]
}

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap_or_else(|| panic!("{name}")))
        .collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4532_4c45_4741,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn islands() -> GameState {
    game(&["Island"; 20])
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
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(id),
        Zone::Battlefield => seat.battlefield.push(id),
        Zone::Graveyard => seat.graveyard.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
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

/// Settles, answering every library or surveil selection by finishing with
/// whatever is already selected, every option with the first, and every
/// trigger target with the first legal one.
fn settle_declining_selections(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Decision::ChooseEffectOption { .. } => act(state, Action::ChooseEffectOption(0)),
            Decision::ChooseEffectBoolean { .. } => act(state, Action::ChooseEffectBoolean(false)),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}

fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            ..
        } => (castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
}

/// Casts `spell`, answering each target prompt from `targets` in order.
fn cast(state: &mut GameState, spell: ObjectId, targets: &[Target]) {
    assert!(
        castable(state, spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    for target in targets {
        choose_target(state, *target);
    }
}

fn choose_target(state: &mut GameState, target: Target) {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Activates `source`'s ability `index`, answering each target prompt.
fn activate(state: &mut GameState, source: ObjectId, index: u8, targets: &[Target]) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    for target in targets {
        choose_target(state, *target);
    }
}

fn power_toughness(state: &GameState, id: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, id),
        engine::effective_toughness(state, id),
    )
}

fn named_on_battlefield(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| CARD_DEFS[state.objects.get(id).card_def as usize].name == name)
        .collect()
}

/// Passes every priority window, declaring no attackers or blockers, until
/// `stop` holds before a decision.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..400 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(Vec::new())),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(Vec::new())),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::Discard { count, choices, .. } => act(
                state,
                Action::Discard(choices.into_iter().take(count as usize).collect()),
            ),
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition never held");
}

/// P1 casts Lightning Bolt at P0 and passes priority to P0.
fn opponent_bolts(state: &mut GameState) -> ObjectId {
    state.priority_player = P1;
    let bolt = put(state, P1, "Lightning Bolt", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, bolt, &[Target::Player(P0)]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    bolt
}

/// Puts one card of each of four card types in P0's graveyard (delirium).
fn delirium(state: &mut GameState) {
    for name in ["Island", "Lightning Bolt", "Ponder", "Mishra's Bauble"] {
        put(state, P0, name, Zone::Graveyard);
    }
}

#[test]
fn e2_cards_are_full_with_their_printed_characteristics() {
    for name in E2_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
    for (name, subtypes) in [
        ("Underground Sea", [Subtype::Island, Subtype::Swamp]),
        ("Volcanic Island", [Subtype::Island, Subtype::Mountain]),
        ("Undercity Sewers", [Subtype::Island, Subtype::Swamp]),
        ("Thundering Falls", [Subtype::Island, Subtype::Mountain]),
    ] {
        let land = def(name);
        assert!(land.is_land, "{name}");
        assert_eq!(land.subtypes, &subtypes, "{name}");
    }
    assert!(!def("Underground Sea").enters_battlefield_tapped);
    assert!(!def("Volcanic Island").enters_battlefield_tapped);
    assert!(def("Undercity Sewers").enters_battlefield_tapped);
    assert!(def("Thundering Falls").enters_battlefield_tapped);

    let borrower = def("Brazen Borrower");
    assert!(borrower.keywords.has(Keywords::FLASH));
    assert!(borrower.keywords.has(Keywords::FLYING));
    assert_eq!(borrower.subtypes, &[Subtype::Faerie, Subtype::Rogue]);
    assert_eq!((borrower.power, borrower.toughness), (Some(3), Some(1)));
    let theft = borrower.adventure.as_ref().expect("Petty Theft");
    assert_eq!(theft.name, "Petty Theft");
    assert_eq!(theft.types, &[CardType::Instant]);

    let murktide = def("Murktide Regent");
    assert!(murktide.delve);
    assert!(murktide.keywords.has(Keywords::FLYING));
    assert_eq!(murktide.mana_value, 7);
    assert!(def("Moonshadow").keywords.has(Keywords::MENACE));
    assert_eq!(
        (def("Moonshadow").power, def("Moonshadow").toughness),
        (Some(7), Some(7))
    );
    assert!(def("Nethergoyf").escape.is_some());
    assert_eq!(def("Nethergoyf").subtypes, &[Subtype::Lhurgoyf]);
    let bowmasters = def("Orcish Bowmasters");
    assert!(bowmasters.keywords.has(Keywords::FLASH));
    assert_eq!(bowmasters.subtypes, &[Subtype::Orc, Subtype::Archer]);
    let army = def("Orc Army Token");
    assert!(army.is_token);
    assert_eq!(army.subtypes, &[Subtype::Orc, Subtype::Army]);
    assert_eq!(army.colors, &[ManaColor::B]);
    assert_eq!((army.power, army.toughness), (Some(0), Some(0)));
    assert_eq!(
        def("Dragon's Rage Channeler").subtypes,
        &[Subtype::Human, Subtype::Shaman]
    );
    assert!(def("Daze").alt_cost.is_some());
    assert!(def("Force of Will").alt_cost.is_some());
    assert!(def("Prismari Charm").mode2.is_some());
    assert!(def("Prismari Charm").mode3.is_some());
    assert_eq!(def("Mishra's Bauble").mana_value, 0);
    for subtype in [Subtype::Army, Subtype::Lhurgoyf, Subtype::Halfling] {
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }
    assert!(!Subtype::Tamiyo.is_creature_type());
}

// ---- Lands ------------------------------------------------------------------

#[test]
fn original_duals_tap_for_either_color_and_have_both_land_types() {
    let mut state = islands();
    let sea = put(&mut state, P0, "Underground Sea", Zone::Battlefield);
    let volcanic = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    for (land, subtype) in [
        (sea, Subtype::Island),
        (sea, Subtype::Swamp),
        (volcanic, Subtype::Island),
        (volcanic, Subtype::Mountain),
    ] {
        assert!(engine::has_effective_subtype(&state, land, subtype));
    }
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(sea, ManaColor::B),
    );
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(volcanic, ManaColor::R),
    );
    let mut expected = [0; 6];
    expected[ManaColor::B.pool_index()] = 1;
    expected[ManaColor::R.pool_index()] = 1;
    assert_eq!(state.players[0].mana_pool, expected);
    assert!(state.objects.get(sea).tapped && state.objects.get(volcanic).tapped);

    let mut state = islands();
    let sea = put(&mut state, P0, "Underground Sea", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(sea, ManaColor::U),
    );
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);
}

#[test]
fn surveil_lands_enter_tapped_and_surveil_one() {
    for name in ["Undercity Sewers", "Thundering Falls"] {
        let mut state = game(&["Lightning Bolt", "Island", "Island", "Island"]);
        let top = state.players[0].library[0];
        let land = put(&mut state, P0, name, Zone::Hand);
        next(&mut state);
        act(&mut state, Action::PlayLand(land));
        assert!(state.objects.get(land).tapped, "{name}");
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert_eq!(legal_targets, vec![Target::Object(top)], "{name}");
                act(&mut state, Action::ChooseEffectTarget(Target::Object(top)));
            }
            other => panic!("{name}: expected the surveil, got {other:?}"),
        }
        settle_declining_selections(&mut state);
        assert_eq!(state.objects.get(top).zone, Zone::Graveyard, "{name}");
    }
}

#[test]
fn fetchlands_pay_one_life_and_fetch_either_land_type_untapped() {
    let mut state = game(&[
        "Forest",
        "Underground Sea",
        "Island",
        "Mountain",
        "Swamp",
        "Volcanic Island",
    ]);
    let library = state.players[0].library.clone();
    let (sea, island, swamp, volcanic) = (library[1], library[2], library[4], library[5]);
    let delta = put(&mut state, P0, "Polluted Delta", Zone::Battlefield);
    activate(&mut state, delta, 0, &[]);
    next(&mut state);
    assert_eq!(state.objects.get(delta).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 19);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets {
            legal_targets,
            max_targets,
            ..
        }) => {
            assert_eq!(max_targets, 1);
            let mut legal = legal_targets.clone();
            legal.sort_by_key(|target| match target {
                Target::Object(id) => id.0,
                _ => u32::MAX,
            });
            let mut expected = vec![sea, island, swamp, volcanic]
                .into_iter()
                .map(Target::Object)
                .collect::<Vec<_>>();
            expected.sort_by_key(|target| match target {
                Target::Object(id) => id.0,
                _ => u32::MAX,
            });
            assert_eq!(legal, expected, "an Island or Swamp card, basic or not");
            act(&mut state, Action::ChooseEffectTarget(Target::Object(sea)));
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(sea).zone, Zone::Battlefield);
    assert!(
        !state.objects.get(sea).tapped,
        "fetched lands enter untapped"
    );
    assert_eq!(state.players[0].library.len(), 5);
}

#[test]
fn every_fetchland_names_its_two_land_types() {
    for (fetch, finds, misses) in [
        ("Bloodstained Mire", ["Swamp", "Mountain"], "Island"),
        ("Flooded Strand", ["Plains", "Island"], "Swamp"),
        ("Marsh Flats", ["Plains", "Swamp"], "Island"),
        ("Misty Rainforest", ["Forest", "Island"], "Swamp"),
        ("Polluted Delta", ["Island", "Swamp"], "Mountain"),
        ("Scalding Tarn", ["Island", "Mountain"], "Forest"),
    ] {
        let mut state = game(&[finds[0], misses, finds[1]]);
        let library = state.players[0].library.clone();
        let land = put(&mut state, P0, fetch, Zone::Battlefield);
        activate(&mut state, land, 0, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert_eq!(legal_targets.len(), 2, "{fetch}");
                assert!(
                    legal_targets.contains(&Target::Object(library[0])),
                    "{fetch}"
                );
                assert!(
                    legal_targets.contains(&Target::Object(library[2])),
                    "{fetch}"
                );
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(library[2])),
                );
            }
            other => panic!("{fetch}: expected the search, got {other:?}"),
        }
        settle_declining_selections(&mut state);
        assert_eq!(state.objects.get(library[2]).zone, Zone::Battlefield);
    }
}

#[test]
fn a_fetched_surveil_land_enters_tapped_and_surveils() {
    let mut state = game(&["Island", "Undercity Sewers", "Forest"]);
    let sewers = state.players[0].library[1];
    let mire = put(&mut state, P0, "Bloodstained Mire", Zone::Battlefield);
    activate(&mut state, mire, 0, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(sewers)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(sewers)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(sewers).zone, Zone::Battlefield);
    assert!(state.objects.get(sewers).tapped);
}

#[test]
fn wasteland_taps_for_colorless_and_destroys_a_nonbasic_land() {
    let mut state = islands();
    let waste = put(&mut state, P0, "Wasteland", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(waste));
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);

    let mut state = islands();
    let waste = put(&mut state, P0, "Wasteland", Zone::Battlefield);
    let sea = put(&mut state, P1, "Underground Sea", Zone::Battlefield);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let tarn = put(&mut state, P1, "Scalding Tarn", Zone::Battlefield);
    assert!(activatable(&mut state, waste, 0));
    act(&mut state, Action::ActivateAbility(waste, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(sea)));
            assert!(legal_targets.contains(&Target::Object(tarn)));
            assert!(!legal_targets.contains(&Target::Object(island)));
            act(&mut state, Action::ChooseTarget(Target::Object(sea)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(waste).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(sea).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(island).zone, Zone::Battlefield);
}

// ---- Thoughtseize, Fatal Push, Unholy Heat -----------------------------------

#[test]
fn thoughtseize_takes_a_nonland_card_and_costs_two_life() {
    let mut state = islands();
    let seize = put(&mut state, P0, "Thoughtseize", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let land = put(&mut state, P1, "Island", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, seize, &[Target::Player(P1)]);
    if let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseEffectTargets {
                player,
                legal_targets,
                ..
            } => {
                assert_eq!(player, P0);
                assert_eq!(legal_targets, vec![Target::Object(bolt)]);
                act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
            }
            other => panic!("expected the discard choice, got {other:?}"),
        }
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(land).zone, Zone::Hand);
    assert_eq!(state.players[0].life, 18);
    assert!(state
        .known_hand_cards(P0, P1)
        .iter()
        .any(|entry| entry.object == land));
}

#[test]
fn thoughtseize_may_target_its_caster() {
    let mut state = islands();
    let seize = put(&mut state, P0, "Thoughtseize", Zone::Hand);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, seize, &[Target::Player(P0)]);
    if let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert_eq!(legal_targets, vec![Target::Object(bolt)]);
                act(&mut state, Action::ChooseEffectTarget(Target::Object(bolt)));
            }
            other => panic!("expected the discard choice, got {other:?}"),
        }
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 18);
}

#[test]
fn fatal_push_destroys_mana_value_two_or_four_with_revolt() {
    // Mana value 2: destroyed.
    let mut state = islands();
    let push = put(&mut state, P0, "Fatal Push", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, push, &[Target::Object(strix)]);
    settled(&mut state);
    assert_eq!(state.objects.get(strix).zone, Zone::Graveyard);

    // Mana value 3 without revolt: survives.
    let mut state = islands();
    let push = put(&mut state, P0, "Fatal Push", Zone::Hand);
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, push, &[Target::Object(borrower)]);
    settled(&mut state);
    assert_eq!(state.objects.get(borrower).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(push).zone, Zone::Graveyard);

    // Revolt: a Bauble left the battlefield under P0's control this turn.
    let mut state = islands();
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let push = put(&mut state, P0, "Fatal Push", Zone::Hand);
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    activate(&mut state, bauble, 0, &[Target::Player(P1)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, push, &[Target::Object(borrower)]);
    settled(&mut state);
    assert_eq!(state.objects.get(borrower).zone, Zone::Graveyard);

    // Mana value 7 is out of reach even with revolt.
    let mut state = islands();
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let push = put(&mut state, P0, "Fatal Push", Zone::Hand);
    let murktide = put(&mut state, P1, "Murktide Regent", Zone::Battlefield);
    activate(&mut state, bauble, 0, &[Target::Player(P1)]);
    settled(&mut state);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, push, &[Target::Object(murktide)]);
    settled(&mut state);
    assert_eq!(state.objects.get(murktide).zone, Zone::Battlefield);
}

#[test]
fn unholy_heat_deals_two_or_six_with_delirium() {
    let mut state = islands();
    let heat = put(&mut state, P0, "Unholy Heat", Zone::Hand);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, heat, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.objects.get(angler).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(angler).damage, 2);

    let mut state = islands();
    delirium(&mut state);
    let heat = put(&mut state, P0, "Unholy Heat", Zone::Hand);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, heat, &[Target::Object(angler)]);
    settled(&mut state);
    assert_eq!(state.objects.get(angler).zone, Zone::Graveyard);
}

// ---- Daze and Force of Will ---------------------------------------------------

#[test]
fn daze_returns_an_island_to_counter_unless_its_controller_pays_one() {
    let mut state = islands();
    let daze = put(&mut state, P0, "Daze", Zone::Hand);
    let sea = put(&mut state, P0, "Underground Sea", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state);
    // One land cannot pay {1}{U}: only the alternative cost is payable.
    cast(&mut state, daze, &[Target::Object(bolt)]);
    next(&mut state);
    assert_eq!(
        state.objects.get(sea).zone,
        Zone::Hand,
        "an Island returned"
    );
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(daze).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "countered: P1 could not pay");

    // P1 pays {1}: the bolt resolves.
    let mut state = islands();
    let daze = put(&mut state, P0, "Daze", Zone::Hand);
    put(&mut state, P0, "Island", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state);
    cast(&mut state, daze, &[Target::Object(bolt)]);
    next(&mut state);
    pool(&mut state, P1, &[], 1);
    while let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, P1);
                act(&mut state, Action::ChooseEffectBoolean(true));
            }
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    assert_eq!(state.players[0].life, 17, "P1 paid and the bolt resolved");
}

#[test]
fn daze_chooses_its_island_and_its_cost() {
    let mut state = islands();
    let daze = put(&mut state, P0, "Daze", Zone::Hand);
    let first = put(&mut state, P0, "Island", Zone::Battlefield);
    let second = put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    let bolt = opponent_bolts(&mut state);
    cast(&mut state, daze, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCastMode { options, .. } => {
            assert!(options.contains(&CastMode::Normal));
            assert!(options.contains(&CastMode::Alternative));
            act(&mut state, Action::ChooseCastMode(CastMode::Alternative));
        }
        other => panic!("expected the cost choice, got {other:?}"),
    }
    match next(&mut state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
            assert_eq!(candidates.len(), 2);
            assert!(candidates.contains(&first) && candidates.contains(&second));
            act(&mut state, Action::ChooseCostTarget(second));
        }
        other => panic!("expected the Island choice, got {other:?}"),
    }
    next(&mut state);
    assert_eq!(state.objects.get(second).zone, Zone::Hand);
    assert_eq!(state.objects.get(first).zone, Zone::Battlefield);
    assert!(!state.objects.get(first).tapped);
}

#[test]
fn force_of_will_pitches_a_blue_card_and_one_life() {
    let mut state = islands();
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let red = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let bolt = opponent_bolts(&mut state);
    cast(&mut state, force, &[Target::Object(bolt)]);
    next(&mut state);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Exile);
    assert_eq!(state.objects.get(red).zone, Zone::Hand);
    assert_eq!(state.players[0].life, 19);
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 19, "the bolt was countered");

    // Two blue cards: the caster chooses which one to exile.
    let mut state = islands();
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    let brainstorm = put(&mut state, P0, "Brainstorm", Zone::Hand);
    let ponder = put(&mut state, P0, "Ponder", Zone::Hand);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let bolt = opponent_bolts(&mut state);
    cast(&mut state, force, &[Target::Object(bolt)]);
    match next(&mut state) {
        Decision::ChooseCostTargets {
            player,
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(player, P0);
            assert_eq!(cost_kind, CostKind::ExileFromHandV1);
            assert_eq!(candidates.len(), 2);
            assert!(candidates.contains(&brainstorm) && candidates.contains(&ponder));
            act(&mut state, Action::ChooseCostTarget(ponder));
        }
        other => panic!("expected the pitch choice, got {other:?}"),
    }
    next(&mut state);
    assert_eq!(state.objects.get(ponder).zone, Zone::Exile);
    assert_eq!(state.objects.get(brainstorm).zone, Zone::Hand);
}

#[test]
fn force_of_will_needs_another_blue_card() {
    let mut state = islands();
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    state.priority_player = P1;
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    assert!(matches!(
        next(&mut state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(&mut state, Action::Pass);
    assert!(!castable(&mut state, force), "no other blue card to exile");
}

// ---- Prismari Charm -----------------------------------------------------------

fn prismari(state: &mut GameState) -> ObjectId {
    let charm = put(state, P0, "Prismari Charm", Zone::Hand);
    pool(state, P0, &[ManaColor::U, ManaColor::R], 0);
    assert!(castable(state, charm));
    act(state, Action::CastSpell(charm));
    charm
}

fn choose_mode(state: &mut GameState, mode: u8) {
    match next(state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&mode), "{legal_modes:?}");
            act(state, Action::ChooseSpellMode(mode));
        }
        other => panic!("expected the mode, got {other:?}"),
    }
}

#[test]
fn prismari_charm_surveils_two_then_draws() {
    let mut state = game(&["Lightning Bolt", "Ponder", "Island", "Island"]);
    let library = state.players[0].library.clone();
    let creature = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let _ = creature;
    prismari(&mut state);
    choose_mode(&mut state, 0);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert!(legal_targets.contains(&Target::Object(library[0])));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(library[0])),
            );
        }
        other => panic!("expected the surveil, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(library[0]).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1, "then draws a card");
}

#[test]
fn prismari_charm_deals_one_damage_to_each_of_one_or_two_targets() {
    let mut state = islands();
    let seer = put(&mut state, P1, "Faerie Seer", Zone::Battlefield);
    prismari(&mut state);
    choose_mode(&mut state, 1);
    choose_target(&mut state, Target::Player(P1));
    choose_target(&mut state, Target::Object(seer));
    settled(&mut state);
    assert_eq!(state.players[1].life, 19);
    assert_eq!(state.objects.get(seer).zone, Zone::Graveyard);

    // One target is enough.
    let mut state = islands();
    prismari(&mut state);
    choose_mode(&mut state, 1);
    choose_target(&mut state, Target::Player(P1));
    match next(&mut state) {
        Decision::ChooseTargets {
            can_finish,
            legal_targets,
            ..
        } => {
            assert!(can_finish);
            assert!(!legal_targets.contains(&Target::Player(P1)), "distinct");
            act(&mut state, Action::FinishEffectSelection);
        }
        Decision::CastSpellOrPass { .. } => {}
        other => panic!("expected the optional second target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.players[1].life, 19);
}

#[test]
fn prismari_charm_bounces_a_nonland_permanent() {
    let mut state = islands();
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    prismari(&mut state);
    choose_mode(&mut state, 2);
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(!legal_targets.contains(&Target::Object(land)));
            act(&mut state, Action::ChooseTarget(Target::Object(strix)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(strix).zone, Zone::Hand);
}

// ---- Brazen Borrower ------------------------------------------------------------

#[test]
fn petty_theft_bounces_then_the_borrower_is_cast_from_exile_with_flash() {
    let mut state = islands();
    let borrower = put(&mut state, P0, "Brazen Borrower", Zone::Hand);
    let strix = put(&mut state, P1, "Baleful Strix", Zone::Battlefield);
    let own = put(&mut state, P0, "Faerie Seer", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::U], 1);
    assert!(castable(&mut state, borrower));
    act(&mut state, Action::CastSpell(borrower));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(strix)));
            assert!(
                !legal_targets.contains(&Target::Object(own)),
                "an opponent's permanent"
            );
            act(&mut state, Action::ChooseTarget(Target::Object(strix)));
        }
        other => panic!("expected Petty Theft's target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(strix).zone, Zone::Hand);
    assert_eq!(state.objects.get(borrower).zone, Zone::Exile);
    assert!(state.objects.get(borrower).v4.on_adventure);

    // On the opponent's turn, with flash, from exile.
    state.active_player = P1;
    state.priority_player = P0;
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 1);
    assert!(castable(&mut state, borrower));
    act(&mut state, Action::CastSpell(borrower));
    settled(&mut state);
    assert_eq!(state.objects.get(borrower).zone, Zone::Battlefield);
    assert_eq!(power_toughness(&state, borrower), (3, 1));
    assert!(engine::has_effective_keyword(
        &state,
        borrower,
        Keywords::FLYING
    ));
}

#[test]
fn brazen_borrower_blocks_only_fliers() {
    let mut state = islands();
    let ground = put(&mut state, P0, "Gurmag Angler", Zone::Battlefield);
    let flier = put(&mut state, P0, "Faerie Seer", Zone::Battlefield);
    let borrower = put(&mut state, P1, "Brazen Borrower", Zone::Battlefield);
    let blocker = put(&mut state, P1, "Thorn of the Black Rose", Zone::Battlefield);
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    match next(&mut state) {
        Decision::DeclareAttackers { .. } => {
            act(&mut state, Action::DeclareAttackers(vec![ground, flier]))
        }
        other => panic!("expected attacks, got {other:?}"),
    }
    let mut seen = false;
    for _ in 0..20 {
        match next(&mut state) {
            Decision::DeclareBlockers { legal_blockers, .. } => {
                let for_ground = &legal_blockers
                    .iter()
                    .find(|(attacker, _)| *attacker == ground)
                    .unwrap()
                    .1;
                let for_flier = &legal_blockers
                    .iter()
                    .find(|(attacker, _)| *attacker == flier)
                    .unwrap()
                    .1;
                assert!(!for_ground.contains(&borrower));
                assert!(for_ground.contains(&blocker));
                assert!(for_flier.contains(&borrower));
                seen = true;
                break;
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(seen, "blocks were declared");
}

// ---- Baubles ------------------------------------------------------------------

#[test]
fn mishras_bauble_looks_privately_and_draws_at_the_next_upkeep() {
    let mut state = islands();
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Battlefield);
    let top = state.players[1].library[0];
    activate(&mut state, bauble, 0, &[Target::Player(P1)]);
    next(&mut state);
    assert_eq!(state.objects.get(bauble).zone, Zone::Graveyard);
    settled(&mut state);
    assert!(state
        .known_library_cards(P0, P1)
        .iter()
        .any(|entry| entry.position == 0 && entry.object == top));
    assert!(state.known_library_cards(P1, P1).is_empty());
    let hand = state.players[0].hand.len();
    assert_eq!(hand, 0, "no draw yet");
    pass_until(&mut state, |state| state.players[0].hand.len() > hand);
    assert_eq!(state.active_player, P1, "the next turn");
    assert_eq!(state.step, Step::Upkeep);
    assert_eq!(state.players[0].hand.len(), 1);
}

#[test]
fn urzas_bauble_looks_at_a_random_hand_card_and_draws_next_upkeep() {
    let mut state = islands();
    let bauble = put(&mut state, P0, "Urza's Bauble", Zone::Battlefield);
    let hand = [
        put(&mut state, P1, "Lightning Bolt", Zone::Hand),
        put(&mut state, P1, "Ponder", Zone::Hand),
        put(&mut state, P1, "Island", Zone::Hand),
    ];
    activate(&mut state, bauble, 0, &[Target::Player(P1)]);
    settled(&mut state);
    let known = state.known_hand_cards(P0, P1);
    assert_eq!(known.len(), 1, "exactly one card is seen");
    assert!(hand.contains(&known[0].object));
    assert!(state.known_hand_cards(P1, P0).is_empty());
    pass_until(&mut state, |state| !state.players[0].hand.is_empty());
    assert_eq!(state.active_player, P1);
    assert_eq!(state.step, Step::Upkeep);
}

// ---- Dragon's Rage Channeler ------------------------------------------------------

#[test]
fn dragons_rage_channeler_surveils_on_noncreature_spells() {
    let mut state = game(&["Island", "Island", "Island"]);
    let top = state.players[0].library[0];
    let channeler = put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(top)]);
            act(&mut state, Action::ChooseEffectTarget(Target::Object(top)));
        }
        other => panic!("expected the surveil, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(top).zone, Zone::Graveyard);
    assert_eq!(state.players[1].life, 17);
    assert_eq!(power_toughness(&state, channeler), (1, 1));

    // A creature spell does not trigger it.
    let mut state = islands();
    put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
    let seer = put(&mut state, P0, "Faerie Seer", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, seer, &[]);
    settle_declining_selections(&mut state);
    assert_eq!(state.players[0].graveyard.len(), 0);
}

#[test]
fn dragons_rage_channeler_has_delirium() {
    let mut state = islands();
    let channeler = put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
    assert_eq!(power_toughness(&state, channeler), (1, 1));
    assert!(!engine::has_effective_keyword(
        &state,
        channeler,
        Keywords::FLYING
    ));
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert!(
        engine::step(&mut state, Action::DeclareAttackers(Vec::new())).is_ok(),
        "no attack requirement without delirium"
    );

    let mut state = islands();
    delirium(&mut state);
    let channeler = put(&mut state, P0, "Dragon's Rage Channeler", Zone::Battlefield);
    assert_eq!(power_toughness(&state, channeler), (3, 3));
    assert!(engine::has_effective_keyword(
        &state,
        channeler,
        Keywords::FLYING
    ));
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    assert!(
        engine::step(&mut state, Action::DeclareAttackers(Vec::new())).is_err(),
        "it attacks each combat if able"
    );
    act(&mut state, Action::DeclareAttackers(vec![channeler]));
}

// ---- Murktide Regent ------------------------------------------------------------

#[test]
fn murktide_regent_counts_delved_instants_and_sorceries_and_grows() {
    let mut state = islands();
    for name in [
        "Lightning Bolt",
        "Ponder",
        "Island",
        "Brainstorm",
        "Counterspell",
    ] {
        put(&mut state, P0, name, Zone::Graveyard);
    }
    let murktide = put(&mut state, P0, "Murktide Regent", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::U], 0);
    cast(&mut state, murktide, &[]);
    next(&mut state);
    assert!(state.players[0].graveyard.is_empty(), "Delve exiled five");
    settled(&mut state);
    assert_eq!(state.objects.get(murktide).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(murktide).counters.plus1_plus1, 4);
    assert_eq!(power_toughness(&state, murktide), (7, 7));
    assert!(engine::has_effective_keyword(
        &state,
        murktide,
        Keywords::FLYING
    ));

    // An instant leaving P0's graveyard adds a counter; a land does not.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let land = put(&mut state, P0, "Island", Zone::Graveyard);
    let theirs = put(&mut state, P1, "Ponder", Zone::Graveyard);
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(bolt, Zone::Exile));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(land, Zone::Exile));
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(theirs, Zone::Exile));
    settled(&mut state);
    assert_eq!(power_toughness(&state, murktide), (8, 8));
}

// ---- Moonshadow ------------------------------------------------------------------

#[test]
fn moonshadow_enters_with_six_counters_and_sheds_one_per_batch() {
    let mut state = islands();
    let moonshadow = put(&mut state, P0, "Moonshadow", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    cast(&mut state, moonshadow, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 6);
    assert_eq!(power_toughness(&state, moonshadow), (1, 1));
    assert!(engine::has_effective_keyword(
        &state,
        moonshadow,
        Keywords::MENACE
    ));

    // Two permanent cards at once: one trigger.
    let lands = [
        put(&mut state, P0, "Island", Zone::Hand),
        put(&mut state, P0, "Baleful Strix", Zone::Hand),
    ];
    event::propose_and_commit_batch(
        &mut state,
        lands
            .iter()
            .map(|&card| ProposedEvent::zone_change(card, Zone::Graveyard))
            .collect(),
    );
    settled(&mut state);
    assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 5);

    // An instant is not a permanent card; an opponent's graveyard is not
    // P0's.
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(bolt, Zone::Graveyard),
    );
    let theirs = put(&mut state, P1, "Island", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(theirs, Zone::Graveyard),
    );
    settled(&mut state);
    assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 5);
    assert_eq!(power_toughness(&state, moonshadow), (2, 2));
}

// ---- Nethergoyf --------------------------------------------------------------------

#[test]
fn nethergoyf_counts_card_types_in_its_owners_graveyard() {
    let mut state = islands();
    let goyf = put(&mut state, P0, "Nethergoyf", Zone::Battlefield);
    assert_eq!(power_toughness(&state, goyf), (0, 1));
    put(&mut state, P0, "Island", Zone::Graveyard);
    put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    put(&mut state, P0, "Counterspell", Zone::Graveyard);
    put(&mut state, P1, "Ponder", Zone::Graveyard);
    assert_eq!(power_toughness(&state, goyf), (2, 3));
    put(&mut state, P0, "Baleful Strix", Zone::Graveyard);
    assert_eq!(power_toughness(&state, goyf), (4, 5), "artifact creature");
}

#[test]
fn nethergoyf_escapes_by_exiling_cards_with_four_card_types() {
    let mut state = islands();
    let goyf = put(&mut state, P0, "Nethergoyf", Zone::Graveyard);
    let island = put(&mut state, P0, "Island", Zone::Graveyard);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let counterspell = put(&mut state, P0, "Counterspell", Zone::Graveyard);
    let ponder = put(&mut state, P0, "Ponder", Zone::Graveyard);
    let bauble = put(&mut state, P0, "Mishra's Bauble", Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::B], 2);
    assert!(castable(&mut state, goyf), "escape from the graveyard");
    act(&mut state, Action::CastSpell(goyf));
    let mut picks = vec![island, bolt, ponder, bauble].into_iter();
    loop {
        match next(&mut state) {
            Decision::ChooseCostTargets {
                cost_kind,
                candidates,
                ..
            } => {
                assert_eq!(cost_kind, CostKind::ExileFromGraveyard);
                assert!(!candidates.contains(&goyf), "other cards");
                let pick = picks.next().expect("four types reached");
                assert!(candidates.contains(&pick));
                act(&mut state, Action::ChooseCostTarget(pick));
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    // Land, instant, sorcery and artifact: the fourth type ends the picks.
    assert_eq!(picks.next(), None, "all four picks were needed");
    for card in [island, bolt, ponder, bauble] {
        assert_eq!(state.objects.get(card).zone, Zone::Exile);
    }
    assert_eq!(state.objects.get(counterspell).zone, Zone::Graveyard);
    settled(&mut state);
    assert_eq!(state.objects.get(goyf).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert_eq!(power_toughness(&state, goyf), (1, 2), "one instant left");
}

// ---- Orcish Bowmasters --------------------------------------------------------------

fn answer_bowmasters_target(state: &mut GameState, target: Target) {
    match settle(state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert!(legal_targets.contains(&target));
            act(state, Action::ChooseTarget(target));
        }
        other => panic!("expected the Bowmasters target, got {other:?}"),
    }
    settled(state);
}

#[test]
fn orcish_bowmasters_pings_and_amasses_on_entry() {
    let mut state = islands();
    state.active_player = P1;
    state.priority_player = P0;
    let bowmasters = put(&mut state, P0, "Orcish Bowmasters", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 1);
    assert!(castable(&mut state, bowmasters), "flash");
    act(&mut state, Action::CastSpell(bowmasters));
    answer_bowmasters_target(&mut state, Target::Player(P1));
    assert_eq!(state.players[1].life, 19);
    let armies = named_on_battlefield(&state, P0, "Orc Army Token");
    assert_eq!(armies.len(), 1);
    assert_eq!(power_toughness(&state, armies[0]), (1, 1));

    // A second amass grows the same Army.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P1));
    answer_bowmasters_target(&mut state, Target::Player(P1));
    assert_eq!(state.players[1].life, 18);
    assert_eq!(named_on_battlefield(&state, P0, "Orc Army Token"), armies);
    assert_eq!(power_toughness(&state, armies[0]), (2, 2));
}

#[test]
fn orcish_bowmasters_ignores_the_draw_step_draw_and_its_own_controller() {
    let mut state = islands();
    put(&mut state, P0, "Orcish Bowmasters", Zone::Battlefield);
    // P0's own draws never trigger it.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    settled(&mut state);
    assert_eq!(state.players[1].life, 20);

    // P1's turn: the turn-based draw is the first draw of the draw step.
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Upkeep;
    pass_until(&mut state, |state| state.step == Step::Draw);
    settled(&mut state);
    assert_eq!(state.step, Step::Draw);
    assert_eq!(state.players[1].draws_this_turn, 1);
    assert_eq!(state.players[1].life, 20);
    assert!(named_on_battlefield(&state, P0, "Orc Army Token").is_empty());

    // A second draw in the same draw step triggers it.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P1));
    answer_bowmasters_target(&mut state, Target::Player(P1));
    assert_eq!(state.players[1].life, 19);
}

// ---- Tamiyo, Inquisitive Student // Tamiyo, Seasoned Scholar --------------------

/// Tamiyo on P0's battlefield, transformed by P0's third draw this turn.
fn transformed_tamiyo() -> (GameState, ObjectId) {
    let mut state = islands();
    let tamiyo = put(
        &mut state,
        P0,
        "Tamiyo, Inquisitive Student",
        Zone::Battlefield,
    );
    for _ in 0..3 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    }
    settled(&mut state);
    (state, tamiyo)
}

#[test]
fn tamiyo_is_a_flying_moonfolk_with_a_planeswalker_back_face() {
    let tamiyo = def("Tamiyo, Inquisitive Student");
    assert_eq!(tamiyo.capability, CardCapability::Full);
    assert_eq!(tamiyo.subtypes, &[Subtype::Moonfolk, Subtype::Wizard]);
    assert_eq!((tamiyo.power, tamiyo.toughness), (Some(0), Some(3)));
    assert!(tamiyo.keywords.has(Keywords::FLYING));
    let back = tamiyo.transform_face.as_ref().expect("back face");
    assert_eq!(back.name, "Tamiyo, Seasoned Scholar");
    assert_eq!(back.types, &[CardType::Planeswalker]);
    assert_eq!(back.subtypes, &[Subtype::Tamiyo]);
    assert_eq!(tamiyo.starting_loyalty, Some(2));
}

#[test]
fn tamiyo_investigates_when_she_attacks() {
    let mut state = islands();
    let tamiyo = put(
        &mut state,
        P0,
        "Tamiyo, Inquisitive Student",
        Zone::Battlefield,
    );
    pass_until(&mut state, |state| state.step == Step::DeclareAttackers);
    act(&mut state, Action::DeclareAttackers(vec![tamiyo]));
    settled(&mut state);
    assert_eq!(named_on_battlefield(&state, P0, "Clue Token").len(), 1);
}

#[test]
fn tamiyo_transforms_on_the_third_draw_of_a_turn() {
    let mut state = islands();
    let tamiyo = put(
        &mut state,
        P0,
        "Tamiyo, Inquisitive Student",
        Zone::Battlefield,
    );
    for _ in 0..2 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(P0));
    }
    settled(&mut state);
    assert_eq!(state.objects.get(tamiyo).v4.face_index, 0, "two draws");
    // The opponent's draws are not hers.
    event::propose_and_commit(&mut state, ProposedEvent::draw(P1));
    settled(&mut state);
    assert_eq!(state.objects.get(tamiyo).v4.face_index, 0);

    let (state, tamiyo) = transformed_tamiyo();
    let live = state.objects.get(tamiyo);
    assert_eq!(live.zone, Zone::Battlefield);
    assert_eq!(live.v4.face_index, 1);
    assert_eq!(live.controller, P0);
    assert!(engine::object_has_type(
        &state,
        tamiyo,
        CardType::Planeswalker
    ));
    assert!(!engine::object_has_type(&state, tamiyo, CardType::Creature));
    assert_eq!(
        mtg_kernel::planeswalker_v1::loyalty(&state, tamiyo),
        Some(2)
    );
}

#[test]
fn tamiyo_plus_two_shrinks_each_creature_attacking_her_controller() {
    let (mut state, tamiyo) = transformed_tamiyo();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    activate(&mut state, tamiyo, 0, &[]);
    settled(&mut state);
    assert_eq!(
        mtg_kernel::planeswalker_v1::loyalty(&state, tamiyo),
        Some(4)
    );

    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::DeclareAttackers
    });
    act(&mut state, Action::DeclareAttackers(vec![angler]));
    loop {
        match next(&mut state) {
            Decision::ChooseAttackTarget { candidates, .. } => {
                assert!(candidates.contains(&Target::Player(P0)));
                act(&mut state, Action::ChooseAttackTarget(Target::Player(P0)));
            }
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } if !state.stack.is_empty() => {
                act(&mut state, Action::Pass)
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(
        power_toughness(&state, angler),
        (4, 5),
        "-1/-0 until end of turn"
    );
}

#[test]
fn tamiyo_minus_three_returns_an_instant_and_adds_mana_for_a_green_one() {
    let (mut state, tamiyo) = transformed_tamiyo();
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, tamiyo, 4);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    let land = put(&mut state, P0, "Island", Zone::Graveyard);
    act(&mut state, Action::ActivateAbility(tamiyo, 1));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(bolt)));
            assert!(!legal_targets.contains(&Target::Object(land)));
            act(&mut state, Action::ChooseTarget(Target::Object(bolt)));
        }
        other => panic!("expected the target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(bolt).zone, Zone::Hand);
    assert_eq!(
        mtg_kernel::planeswalker_v1::loyalty(&state, tamiyo),
        Some(3)
    );
    let pool_before = state.players[0].mana_pool;
    assert_eq!(pool_before, [0; 6], "a red card adds no mana");

    let (mut state, tamiyo) = transformed_tamiyo();
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, tamiyo, 4);
    let storm = put(&mut state, P0, "Weather the Storm", Zone::Graveyard);
    act(&mut state, Action::ActivateAbility(tamiyo, 1));
    choose_target(&mut state, Target::Object(storm));
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { option_count, .. }) => {
            assert_eq!(option_count, 5);
            act(&mut state, Action::ChooseEffectOption(4));
        }
        other => panic!("expected the color choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(storm).zone, Zone::Hand);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 1);
}

#[test]
fn tamiyo_minus_seven_draws_half_the_library_and_lifts_the_hand_size() {
    let (mut state, tamiyo) = transformed_tamiyo();
    mtg_kernel::planeswalker_v1::change_loyalty(&mut state, tamiyo, 5);
    let library = state.players[0].library.len();
    let hand = state.players[0].hand.len();
    act(&mut state, Action::ActivateAbility(tamiyo, 2));
    settled(&mut state);
    assert_eq!(
        state.players[0].hand.len(),
        hand + library.div_ceil(2),
        "half the library, rounded up"
    );
    assert_eq!(
        state.objects.get(tamiyo).zone,
        Zone::Graveyard,
        "zero loyalty"
    );
    assert!(state.eternal_v1.as_ref().unwrap().no_maximum_hand_size[0]);
    let kept = state.players[0].hand.len();
    assert!(kept > 7);
    pass_until(&mut state, |state| state.active_player == P1);
    assert_eq!(state.players[0].hand.len(), kept, "no discard to seven");
}
