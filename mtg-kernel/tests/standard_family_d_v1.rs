//! MageZero Standard family D: newer set keywords (prowess, valiant, speed,
//! warp, offspring, plot, crime and friends) in the Standard catalog.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::standard_keywords_v1::speed;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, SpeedV1, Step, Target, Zone};

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

fn power_toughness(state: &GameState, id: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, id),
        engine::effective_toughness(state, id),
    )
}

#[test]
fn family_d_cards_are_fully_supported() {
    for name in ["Emberheart Challenger", "Burnout Bashtronaut"] {
        let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            CARD_DEFS[id as usize].capability,
            CardCapability::Full,
            "{name}"
        );
    }
}

#[test]
fn emberheart_challenger_has_prowess_and_valiant() {
    let mut state = ready();
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    assert!(engine::has_effective_keyword(
        &state,
        challenger,
        Keywords::HASTE
    ));
    let first = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Burst Lightning", Zone::Hand);
    let library = state.players[0].library.len();

    // Targeting it with your own noncreature spell triggers both prowess and
    // valiant; prowess resolves first, so the 2 damage doesn't kill it.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, first, &[Target::Object(challenger)]);
    settled(&mut state);
    assert_eq!(state.objects.get(challenger).zone, Zone::Battlefield);
    assert_eq!(power_toughness(&state, challenger), (3, 3));
    assert_eq!(state.objects.get(challenger).damage, 2);
    assert_eq!(state.players[0].library.len(), library - 1);
    let exiled = state.exile.clone();
    assert_eq!(exiled.len(), 1);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { land_drops, .. } if land_drops.contains(&exiled[0]))
    );

    // The second targeting this turn is not the first: only prowess.
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, second, &[Target::Player(PlayerId::P1)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, challenger), (4, 4));
    assert_eq!(state.exile.len(), 1);
    assert_eq!(state.players[1].life, 18);
}

#[test]
fn valiant_ignores_opponents_targeting() {
    let mut state = ready();
    let challenger = put(
        &mut state,
        PlayerId::P0,
        "Emberheart Challenger",
        Zone::Battlefield,
    );
    let burst = put(&mut state, PlayerId::P1, "Burst Lightning", Zone::Hand);
    state.priority_player = PlayerId::P1;
    add_mana(&mut state, PlayerId::P1, &[ManaColor::R], 0);
    cast(&mut state, burst, &[Target::Object(challenger)]);
    settled(&mut state);
    assert_eq!(state.objects.get(challenger).zone, Zone::Graveyard);
    assert!(state.exile.is_empty());
}

fn burn(state: &mut GameState, player: PlayerId, target: Target) {
    let burst = put(state, player, "Burst Lightning", Zone::Hand);
    add_mana(state, player, &[ManaColor::R], 0);
    cast(state, burst, &[target]);
    settled(state);
}

#[test]
fn burnout_bashtronaut_starts_and_raises_speed_once_per_turn() {
    let mut state = ready();
    let bashtronaut = put(&mut state, PlayerId::P0, "Burnout Bashtronaut", Zone::Hand);
    assert_eq!(speed(&state, PlayerId::P0), 0);
    add_mana(&mut state, PlayerId::P0, &[ManaColor::R], 0);
    cast(&mut state, bashtronaut, &[]);
    settled(&mut state);
    assert!(engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::MENACE
    ));
    assert_eq!(speed(&state, PlayerId::P0), 1);
    assert_eq!(speed(&state, PlayerId::P1), 0);

    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2);
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2, "only once each turn");

    // An opponent losing life during the opponent's own turn doesn't count.
    state.turn += 1;
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    burn(&mut state, PlayerId::P1, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 2);
}

#[test]
fn burnout_bashtronaut_pumps_and_has_double_strike_at_max_speed() {
    let mut state = ready();
    let bashtronaut = put(
        &mut state,
        PlayerId::P0,
        "Burnout Bashtronaut",
        Zone::Battlefield,
    );
    mtg_kernel::trigger::sba_fixed_point(&mut state);
    assert_eq!(speed(&state, PlayerId::P0), 1);
    assert!(!engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::DOUBLE_STRIKE
    ));
    state.speed_v1 = Some(SpeedV1 {
        speeds: [4, 0],
        last_increase: None,
    });
    assert!(engine::has_effective_keyword(
        &state,
        bashtronaut,
        Keywords::DOUBLE_STRIKE
    ));
    burn(&mut state, PlayerId::P0, Target::Player(PlayerId::P1));
    assert_eq!(speed(&state, PlayerId::P0), 4, "speed can't exceed four");

    add_mana(&mut state, PlayerId::P0, &[], 2);
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(bashtronaut, 0)))
    );
    engine::step(&mut state, Action::ActivateAbility(bashtronaut, 0)).unwrap();
    settled(&mut state);
    assert_eq!(power_toughness(&state, bashtronaut), (2, 1));
}
