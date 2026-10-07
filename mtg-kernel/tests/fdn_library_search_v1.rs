//! FDN library-search, kicker and flashback reference cards: Campus Guide,
//! Burnished Hart, Grow from the Ashes and Revenge of the Rats (plus its Rat
//! token).
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, preflight_fully_supported_deck, CardCapability, CardType, CostComponent,
    Subtype, CARD_DEFS, KERNEL_CARDDB_HASH,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger::{self, TriggerCondition};

const NAMES: [&str; 5] = [
    "Campus Guide",
    "Burnished Hart",
    "Grow from the Ashes",
    "Revenge of the Rats",
    "Rat Token",
];

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn ready() -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], |_| "Plains".into(), 0x4644_4e4c);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
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
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Library => seat.library.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        _ => panic!("helper zone"),
    }
    object
}

fn mana(state: &mut GameState, player: PlayerId, color: ManaColor, amount: u8) {
    state.players[player.index()].mana_pool[color.pool_index()] = amount;
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

/// Passes priority until the stack is empty or a non-priority decision is
/// pending, returning that decision.
fn settle(state: &mut GameState) -> Decision {
    for _ in 0..60 {
        let decision = next(state);
        match decision {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return decision,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => return other,
        }
    }
    panic!("resolution did not settle");
}

fn settle_to_priority(state: &mut GameState) {
    let decision = settle(state);
    assert!(
        matches!(decision, Decision::CastSpellOrPass { .. }),
        "unexpected decision: {decision:?}"
    );
    assert!(state.engine.pending_triggers.is_empty());
}

/// Picks `picks` in a pending library search and finishes it if the engine
/// still waits for more.
fn pick(state: &mut GameState, picks: &[ObjectId]) {
    for &card in picks {
        engine::step(state, Action::ChooseEffectTarget(Target::Object(card))).unwrap();
    }
    if matches!(next(state), Decision::ChooseEffectTargets { .. }) {
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
}

fn expect_search(state: &mut GameState, max: u16) -> Vec<Target> {
    match settle(state) {
        Decision::ChooseEffectTargets {
            player: PlayerId::P0,
            legal_targets,
            min_targets: 0,
            max_targets,
            can_finish: true,
            ..
        } => {
            assert_eq!(max_targets, max);
            legal_targets
        }
        other => panic!("expected the library search, got {other:?}"),
    }
}

#[test]
fn appended_definitions_match_their_printed_characteristics() {
    let first = id(NAMES[0]);
    for (offset, name) in NAMES.iter().enumerate() {
        let card_def = id(name);
        assert_eq!(card_def, first + offset as u16, "{name} appends in order");
        assert_eq!(
            CARD_DEFS[card_def as usize].capability,
            CardCapability::Full
        );
    }
    assert_eq!(first as usize + NAMES.len(), CARD_DEFS.len());
    preflight_fully_supported_deck(&NAMES[..4].iter().map(|name| id(name)).collect::<Vec<_>>())
        .unwrap();
    println!("FDN catalog hash: {KERNEL_CARDDB_HASH:016x}");

    let guide = &CARD_DEFS[id("Campus Guide") as usize];
    assert_eq!(guide.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(guide.subtypes, &[Subtype::Golem]);
    assert_eq!(
        (guide.cost.generic, guide.power, guide.toughness),
        (2, Some(2), Some(1))
    );
    let triggers = trigger::triggers_for(id("Campus Guide"));
    assert_eq!(triggers.len(), 1);
    assert_eq!(triggers[0].condition, TriggerCondition::Etb);

    let hart = &CARD_DEFS[id("Burnished Hart") as usize];
    assert_eq!(hart.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(hart.subtypes, &[Subtype::Elk]);
    assert_eq!(
        (hart.cost.generic, hart.power, hart.toughness),
        (3, Some(2), Some(2))
    );

    let grow = &CARD_DEFS[id("Grow from the Ashes") as usize];
    assert_eq!(grow.types, &[CardType::Sorcery]);
    assert_eq!(grow.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(grow.cost.generic, 2);
    assert_eq!(
        grow.kicker_cost,
        Some(Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        })
    );

    let revenge = &CARD_DEFS[id("Revenge of the Rats") as usize];
    assert_eq!(revenge.types, &[CardType::Sorcery]);
    assert_eq!(revenge.cost.generic, 2);
    assert_eq!(
        revenge.flashback.as_ref().expect("flashback").cost,
        &[CostComponent::Mana(revenge.cost)]
    );

    let rat = &CARD_DEFS[id("Rat Token") as usize];
    assert!(rat.is_token);
    assert_eq!(rat.subtypes, &[Subtype::Rat]);
    assert_eq!((rat.power, rat.toughness), (Some(1), Some(1)));
}

fn cast_campus_guide(state: &mut GameState) -> ObjectId {
    let guide = put(state, PlayerId::P0, "Campus Guide", Zone::Hand);
    mana(state, PlayerId::P0, ManaColor::C, 2);
    next(state);
    engine::step(state, Action::CastSpell(guide)).unwrap();
    guide
}

#[test]
fn campus_guide_puts_a_revealed_basic_land_on_top_after_shuffling() {
    let mut state = ready();
    let mut basics = Vec::new();
    for name in ["Plains", "Island", "Swamp", "Forest"] {
        basics.push(put(&mut state, PlayerId::P0, name, Zone::Library));
    }
    let hart = put(&mut state, PlayerId::P0, "Burnished Hart", Zone::Library);
    let guide = cast_campus_guide(&mut state);
    let legal = expect_search(&mut state, 1);
    assert_eq!(legal.len(), 4);
    assert!(!legal.contains(&Target::Object(hart)));
    let hash = state.state_hash();
    assert!(engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(hart))).is_err());
    assert_eq!(state.state_hash(), hash);
    let swamp = basics[2];
    pick(&mut state, &[swamp]);
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(guide).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(swamp).zone, Zone::Library);
    assert_eq!(
        state.players[0].library[0], swamp,
        "on top after the shuffle"
    );
    assert_eq!(state.players[0].library.len(), 5);
}

#[test]
fn campus_guide_may_decline_the_search() {
    let mut state = ready();
    let plains = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let guide = cast_campus_guide(&mut state);
    expect_search(&mut state, 1);
    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(guide).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(plains).zone, Zone::Library);
}

fn activate_hart(state: &mut GameState) -> ObjectId {
    let hart = put(state, PlayerId::P0, "Burnished Hart", Zone::Battlefield);
    mana(state, PlayerId::P0, ManaColor::C, 2);
    let decision = next(state);
    assert!(
        matches!(&decision, Decision::CastSpellOrPass { activatable_abilities, .. }
        if !activatable_abilities.contains(&(hart, 0))),
        "the ability needs three mana"
    );
    mana(state, PlayerId::P0, ManaColor::C, 3);
    next(state);
    engine::step(state, Action::ActivateAbility(hart, 0)).unwrap();
    hart
}

#[test]
fn burnished_hart_sacrifices_to_fetch_two_basic_lands_tapped() {
    let mut state = ready();
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Library);
    let island = put(&mut state, PlayerId::P0, "Island", Zone::Library);
    let plains = put(&mut state, PlayerId::P0, "Plains", Zone::Library);
    let guide = put(&mut state, PlayerId::P0, "Campus Guide", Zone::Library);
    let hart = activate_hart(&mut state);
    let legal = expect_search(&mut state, 2);
    assert_eq!(state.objects.get(hart).zone, Zone::Graveyard);
    assert_eq!(legal.len(), 3);
    assert!(!legal.contains(&Target::Object(guide)));
    pick(&mut state, &[forest, island]);
    settle_to_priority(&mut state);
    for land in [forest, island] {
        assert_eq!(state.objects.get(land).zone, Zone::Battlefield);
        assert!(state.objects.get(land).tapped);
    }
    assert_eq!(state.objects.get(plains).zone, Zone::Library);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

#[test]
fn burnished_hart_may_find_fewer_lands() {
    let mut state = ready();
    let forest = put(&mut state, PlayerId::P0, "Forest", Zone::Library);
    let island = put(&mut state, PlayerId::P0, "Island", Zone::Library);
    activate_hart(&mut state);
    expect_search(&mut state, 2);
    pick(&mut state, &[island]);
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(island).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(forest).zone, Zone::Library);
}

/// Casts Grow from the Ashes with `generic` mana beyond {G}, answering the
/// kicker prompt (when offered) with `kick`, then picks every basic offered.
fn cast_grow(generic: u8, kick: bool, expected_max: u16) -> (GameState, Vec<ObjectId>) {
    let mut state = ready();
    let lands: Vec<_> = ["Forest", "Swamp", "Island"]
        .into_iter()
        .map(|name| put(&mut state, PlayerId::P0, name, Zone::Library))
        .collect();
    let grow = put(&mut state, PlayerId::P0, "Grow from the Ashes", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::G, 1);
    mana(&mut state, PlayerId::P0, ManaColor::C, generic);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(grow)).unwrap();
    if let Decision::ChooseKicker { player, spell } = next(&mut state) {
        assert_eq!((player, spell), (PlayerId::P0, grow));
        engine::step(&mut state, Action::ChooseKicker(kick)).unwrap();
    }
    assert_eq!(expect_search(&mut state, expected_max).len(), 3);
    pick(&mut state, &lands[..expected_max as usize]);
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(grow).zone, Zone::Graveyard);
    (state, lands)
}

#[test]
fn grow_from_the_ashes_fetches_one_basic_untapped_or_two_when_kicked() {
    let (state, lands) = cast_grow(2, false, 1);
    assert_eq!(state.objects.get(lands[0]).zone, Zone::Battlefield);
    assert!(!state.objects.get(lands[0]).tapped);
    assert_eq!(state.objects.get(lands[1]).zone, Zone::Library);

    let (state, lands) = cast_grow(4, false, 1);
    assert_eq!(state.objects.get(lands[1]).zone, Zone::Library);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 2);

    let (state, lands) = cast_grow(4, true, 2);
    for land in &lands[..2] {
        assert_eq!(state.objects.get(*land).zone, Zone::Battlefield);
        assert!(!state.objects.get(*land).tapped);
    }
    assert_eq!(state.objects.get(lands[2]).zone, Zone::Library);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
}

fn rats(state: &GameState) -> Vec<ObjectId> {
    state.players[0]
        .battlefield
        .iter()
        .copied()
        .filter(|object| state.objects.get(*object).card_def == id("Rat Token"))
        .collect()
}

#[test]
fn revenge_of_the_rats_makes_tapped_rats_per_creature_card_then_flashes_back() {
    let mut state = ready();
    put(&mut state, PlayerId::P0, "Campus Guide", Zone::Graveyard);
    put(&mut state, PlayerId::P0, "Burnished Hart", Zone::Graveyard);
    put(
        &mut state,
        PlayerId::P0,
        "Grow from the Ashes",
        Zone::Graveyard,
    );
    put(&mut state, PlayerId::P1, "Campus Guide", Zone::Graveyard);
    let revenge = put(&mut state, PlayerId::P0, "Revenge of the Rats", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::B, 2);
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(revenge)).unwrap();
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(revenge).zone, Zone::Graveyard);
    let first = rats(&state);
    assert_eq!(first.len(), 2, "only the caster's creature cards count");
    for rat in &first {
        assert!(state.objects.get(*rat).tapped);
        assert_eq!(state.objects.get(*rat).controller, PlayerId::P0);
    }

    mana(&mut state, PlayerId::P0, ManaColor::B, 2);
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(revenge)).unwrap();
    assert!(state.stack.last().unwrap().is_flashback);
    settle_to_priority(&mut state);
    assert_eq!(state.objects.get(revenge).zone, Zone::Exile);
    assert_eq!(rats(&state).len(), 4);
}

#[test]
fn revenge_of_the_rats_makes_nothing_with_no_creature_cards() {
    let mut state = ready();
    put(
        &mut state,
        PlayerId::P0,
        "Grow from the Ashes",
        Zone::Graveyard,
    );
    let revenge = put(&mut state, PlayerId::P0, "Revenge of the Rats", Zone::Hand);
    mana(&mut state, PlayerId::P0, ManaColor::B, 2);
    mana(&mut state, PlayerId::P0, ManaColor::C, 2);
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(revenge)).unwrap();
    settle_to_priority(&mut state);
    assert!(rats(&state).is_empty());
}
