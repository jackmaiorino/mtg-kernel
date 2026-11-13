//! Premodern sideboard wave P3e: native rules tests for the cards that
//! complete Madness Threshold's 75.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3E_CARDS: [&str; 3] = ["Absolute Law", "Kor Haven", "Winter Orb"];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds twenty Islands, and both hands are empty.
fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4500_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    assert!(state.players.iter().all(|seat| seat.hand.is_empty()));
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
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

/// Puts a tapped permanent onto `player`'s battlefield.
fn put_tapped(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(object).tapped = true;
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn tapped(state: &GameState, object: ObjectId) -> bool {
    state.objects.get(object).tapped
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Creature)
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty. Any other choice fails the test.
fn settled(state: &mut GameState) {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    panic!("did not settle");
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

/// Starts casting `spell` and returns its first target prompt's legal
/// targets.
fn cast_and_list_targets(state: &mut GameState, spell: ObjectId) -> Vec<Target> {
    assert!(
        priority(state).0.contains(&spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
    target_prompt(state)
}

/// Starts activating `source`'s ability `index` and returns its first
/// target prompt's legal targets.
fn activate_and_list_targets(state: &mut GameState, source: ObjectId, index: u8) -> Vec<Target> {
    assert!(
        priority(state).1.contains(&(source, index)),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
    target_prompt(state)
}

fn target_prompt(state: &mut GameState) -> Vec<Target> {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Plays on until `stop` holds before a decision. `answer` may answer a
/// decision; otherwise priority passes, nobody attacks or blocks, and
/// triggers keep their order. Any other choice fails the test.
fn run_until(
    state: &mut GameState,
    stop: impl Fn(&GameState) -> bool,
    mut answer: impl FnMut(&GameState, &Decision) -> Option<Action>,
) {
    for _ in 0..3000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        if let Some(action) = answer(state, &decision) {
            act(state, action);
            continue;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            Decision::ChooseCombatDamageRange { .. } => {
                Action::ChooseCombatDamageRange { upper_half: true }
            }
            Decision::ChooseAttackTarget {
                player, candidates, ..
            } => Action::ChooseAttackTarget(
                if candidates.contains(&Target::Player(player.opponent())) {
                    Target::Player(player.opponent())
                } else {
                    candidates[0]
                },
            ),
            other => panic!("unexpected choice: {other:?}"),
        };
        act(state, action);
    }
    panic!("condition never held");
}

fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3e_cards_are_full_with_their_printed_characteristics() {
    for name in P3E_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    let law = def("Absolute Law");
    assert!(law.has_type(CardType::Enchantment));
    assert_eq!((law.mana_value, law.colors), (2, &[ManaColor::W][..]));
    let haven = def("Kor Haven");
    assert!(haven.has_type(CardType::Land) && haven.is_land);
    assert!(haven.supertypes.contains(&Supertype::Legendary));
    assert_eq!(haven.produces_mana, &[ManaColor::C]);
    assert_eq!(haven.activated_abilities.len(), 1);
    let orb = def("Winter Orb");
    assert!(orb.has_type(CardType::Artifact));
    assert_eq!((orb.mana_value, orb.colors), (2, &[][..]));
}

// ---- Absolute Law ---------------------------------------------------------------

#[test]
fn absolute_law_gives_every_creature_protection_from_red() {
    let mut state = islands();
    put(&mut state, P0, "Absolute Law", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let first_island = put(&mut state, P1, "Island", Zone::Battlefield);
    let second_island = put(&mut state, P1, "Island", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let pyroclasm = put(&mut state, P0, "Pyroclasm", Zone::Hand);
    let first_rain = put(&mut state, P0, "Stone Rain", Zone::Hand);
    let second_rain = put(&mut state, P0, "Stone Rain", Zone::Hand);

    // Red spells can't target creatures, the opponent's included.
    pool(&mut state, P0, &[ManaColor::R], 0);
    let targets = cast_and_list_targets(&mut state, bolt);
    assert!(
        !targets.contains(&Target::Object(elves)),
        "its controller's"
    );
    assert!(!targets.contains(&Target::Object(angler)), "the opponent's");
    assert!(targets.contains(&Target::Player(P1)));
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);

    // Damage from red sources is prevented.
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(pyroclasm));
    settled(&mut state);
    assert_eq!(zone(&state, elves), Zone::Battlefield, "the 1/1 survives");
    assert_eq!(state.objects.get(elves).damage, 0);
    assert_eq!(state.objects.get(angler).damage, 0);

    // A land is protected only while it is a creature.
    pool(&mut state, P0, &[ManaColor::R], 2);
    let targets = cast_and_list_targets(&mut state, first_rain);
    assert!(targets.contains(&Target::Object(factory)), "not a creature");
    act(
        &mut state,
        Action::ChooseTarget(Target::Object(first_island)),
    );
    settled(&mut state);
    assert_eq!(zone(&state, first_island), Zone::Graveyard);
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(factory, 0));
    settled(&mut state);
    assert!(is_creature(&state, factory));
    pool(&mut state, P0, &[ManaColor::R], 2);
    let targets = cast_and_list_targets(&mut state, second_rain);
    assert!(
        !targets.contains(&Target::Object(factory)),
        "a creature now"
    );
    assert!(targets.contains(&Target::Object(second_island)));
    act(
        &mut state,
        Action::ChooseTarget(Target::Object(second_island)),
    );
    settled(&mut state);
    assert_eq!(zone(&state, factory), Zone::Battlefield);
    assert_eq!(zone(&state, second_island), Zone::Graveyard);
}

// ---- Kor Haven ------------------------------------------------------------------

#[test]
fn kor_haven_prevents_the_combat_damage_its_target_would_deal() {
    let mut state = islands();
    let haven = put(&mut state, P0, "Kor Haven", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    run_until(
        &mut state,
        |state| {
            state.active_player == P1
                && state.step == Step::DeclareAttackers
                && state.priority_player == P0
                && state.objects.get(angler).tapped
        },
        |_, decision| match decision {
            Decision::DeclareAttackers { player, .. } if *player == P1 => {
                Some(Action::DeclareAttackers(vec![angler, fanatic]))
            }
            _ => None,
        },
    );

    // Only attacking creatures are targets.
    pool(&mut state, P0, &[ManaColor::W], 1);
    let targets = activate_and_list_targets(&mut state, haven, 0);
    assert!(targets.contains(&Target::Object(angler)));
    assert!(targets.contains(&Target::Object(fanatic)));
    assert!(!targets.contains(&Target::Object(elves)), "not attacking");
    act(&mut state, Action::ChooseTarget(Target::Object(angler)));
    settled(&mut state);
    assert!(tapped(&state, haven));

    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        run_until(
            continued,
            |state| state.active_player == P1 && state.step == Step::Main2,
            |_, decision| match decision {
                Decision::DeclareBlockers { player, .. } if *player == P0 => {
                    Some(Action::DeclareBlockers(vec![(elves, angler)]))
                }
                _ => None,
            },
        );
        // The Angler's damage to the blocking Elves is prevented, the
        // Elves' damage to it is not, and the unblocked Fanatic hits.
        assert_eq!(zone(continued, elves), Zone::Battlefield);
        assert_eq!(continued.objects.get(elves).damage, 0);
        assert_eq!(continued.objects.get(angler).damage, 1, "one way only");
        assert_eq!(continued.players[0].life, 19);
    }

    // The prevention lasts this turn only.
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Main1
    });
    assert!(state.eternal_v1.as_ref().unwrap().p3e.havens.is_empty());
}

// ---- Winter Orb -----------------------------------------------------------------

#[test]
fn winter_orb_untaps_one_land_of_the_active_players_choice() {
    let mut state = islands();
    // The Orb's controller doesn't matter: it limits every player.
    let orb = put(&mut state, P0, "Winter Orb", Zone::Battlefield);
    let forests = [
        put_tapped(&mut state, P1, "Forest"),
        put_tapped(&mut state, P1, "Forest"),
        put_tapped(&mut state, P1, "Forest"),
    ];
    let elves = put_tapped(&mut state, P1, "Llanowar Elves");
    // A land that doesn't untap anyway is no option.
    let skipped = put_tapped(&mut state, P1, "Forest");
    state.objects.get_mut(skipped).v4.skip_next_untap = true;

    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Untap
    });
    match next(&mut state) {
        Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        } => {
            assert_eq!((player, source, option_count), (P1, orb, 3));
        }
        other => panic!("expected Winter Orb's choice, got {other:?}"),
    }
    assert!(forests.iter().all(|&forest| tapped(&state, forest)));
    assert!(!tapped(&state, elves), "other permanents untap as usual");

    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        act(continued, Action::ChooseEffectOption(1));
        assert_eq!(continued.step, Step::Untap, "chosen before the step ends");
        assert!(tapped(continued, forests[0]));
        assert!(!tapped(continued, forests[1]), "the chosen land");
        assert!(tapped(continued, forests[2]));
        assert!(tapped(continued, skipped));
        pass_until(continued, |state| {
            state.active_player == P1 && state.step == Step::Main1
        });
        assert!(tapped(continued, forests[0]) && tapped(continued, forests[2]));
        assert!(continued
            .eternal_v1
            .as_ref()
            .unwrap()
            .p3e
            .orb_untap
            .is_none());
    }
}

#[test]
fn winter_orb_lets_a_lone_land_untap_and_every_land_once_it_is_tapped() {
    let mut state = islands();
    let orb = put_tapped(&mut state, P1, "Winter Orb");
    let forests = [
        put_tapped(&mut state, P1, "Forest"),
        put_tapped(&mut state, P1, "Forest"),
    ];
    let plains = put_tapped(&mut state, P0, "Plains");

    // Tapped as P1's untap step begins, the Orb untaps with every land.
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Upkeep
    });
    assert!(!tapped(&state, orb));
    assert!(forests.iter().all(|&forest| !tapped(&state, forest)));

    // Untapped as P0's untap step begins, it limits P0 to one land, and a
    // single tapped land untaps without a choice.
    assert!(tapped(&state, plains));
    pass_until(&mut state, |state| {
        state.active_player == P0 && state.step == Step::Upkeep
    });
    assert!(!tapped(&state, plains));
}
