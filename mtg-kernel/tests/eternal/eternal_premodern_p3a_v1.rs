//! Premodern sideboard wave P3a: native rules tests for the cards that
//! complete the 75s of Stiflenought, Sligh, Goblins, Oath of Druids, RG
//! Aggro, Stasis and White Weenie.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3A_CARDS: [&str; 11] = [
    "Anarchy",
    "Chill",
    "Crimson Acolyte",
    "Cursed Totem",
    "Essence Flare",
    "Overload",
    "Pyrostatic Pillar",
    "Sphere of Law",
    "Tranquil Domain",
    "True Believer",
    "Warmth",
];

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with P0 active and holding priority. Each library
/// holds twenty Islands.
fn islands() -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5033_4100_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
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
        Zone::Graveyard => seat.graveyard.push(object),
        _ => panic!("helper zone"),
    }
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
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

/// The first legal answer to any choice: the first target or option,
/// finishing a selection that may finish, declining a Boolean or kicker.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            legal_targets,
            can_finish: true,
            ..
        } if legal_targets.is_empty() => Action::FinishEffectSelection,
        Decision::ChooseTargets { legal_targets, .. } => Action::ChooseTarget(legal_targets[0]),
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => Action::FinishEffectSelection,
        Decision::ChooseEffectTargets { legal_targets, .. } => {
            Action::ChooseEffectTarget(legal_targets[0])
        }
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(false),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::ChooseSpellMode { legal_modes, .. } => Action::ChooseSpellMode(legal_modes[0]),
        Decision::ChooseOptionalCost { .. } => {
            Action::ChooseOptionalCost(OptionalCostChoice::Decline)
        }
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
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

/// Whether any of `source`'s activated abilities may be activated now.
fn has_activatable_ability(state: &mut GameState, source: ObjectId) -> bool {
    priority(state)
        .1
        .iter()
        .any(|&(object, _)| object == source)
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

/// Casts the split card `spell` as half `mode`, then its targets.
fn cast_half(state: &mut GameState, spell: ObjectId, mode: u8, targets: &[Target]) {
    assert!(castable(state, spell));
    act(state, Action::CastSpell(spell));
    if let Decision::ChooseSpellMode { legal_modes, .. } = next(state) {
        assert!(legal_modes.contains(&mode), "{mode} not in {legal_modes:?}");
        act(state, Action::ChooseSpellMode(mode));
    }
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

/// The targets `spell` may choose first, cast by its owner now.
fn cast_targets(state: &GameState, spell: ObjectId) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(castable(&mut probe, spell));
    act(&mut probe, Action::CastSpell(spell));
    match next(&mut probe) {
        Decision::ChooseTargets { legal_targets, .. } => legal_targets,
        other => panic!("expected targets, got {other:?}"),
    }
}

/// Plays on until `stop` holds before a decision: passing priority, not
/// attacking or blocking, ordering triggers as given and answering any
/// other choice with the first legal answer.
fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    for _ in 0..2000 {
        let decision = next(state);
        if stop(state) {
            return;
        }
        let action = match decision {
            Decision::CastSpellOrPass { .. } => Action::Pass,
            Decision::DeclareAttackers { .. } => Action::DeclareAttackers(Vec::new()),
            Decision::DeclareBlockers { .. } => Action::DeclareBlockers(Vec::new()),
            Decision::OrderTriggers { pending, .. } => {
                Action::OrderTriggers((0..pending.len()).collect())
            }
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Plays to `player`'s next first main phase after this turn's. The
/// engine's `turn` counts rounds: both players' turns share a number.
fn to_main_phase_of(state: &mut GameState, player: PlayerId) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.active_player == player
            && state.step == Step::Main1
    });
}

/// Passes to the next turn's upkeep with a trigger on the stack.
fn to_next_upkeep_trigger(state: &mut GameState) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.step == Step::Upkeep
            && !state.stack.is_empty()
    });
}

/// Deals `amount` damage from `source` to `target`.
fn hit(state: &mut GameState, source: ObjectId, target: Target, amount: i32) {
    event::propose_and_commit(state, ProposedEvent::damage(source, target, amount));
}

/// Moves `object` to its owner's graveyard, as if it were destroyed.
fn bury(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// Answers the X announcement of a pending cast with `x`.
fn announce_x(state: &mut GameState, x: u16) {
    match next(state) {
        Decision::ChooseEffectOption { option_count, .. } => {
            assert!(x < option_count, "X {x} of {option_count}");
            act(state, Action::ChooseEffectOption(x));
        }
        other => panic!("expected the X announcement, got {other:?}"),
    }
}

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3a_cards_are_full_with_their_printed_characteristics() {
    for name in P3A_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    assert!(def("Overload").kicker_cost.is_some());
    assert!(def("Essence Flare").subtypes.contains(&Subtype::Aura));
    for (name, power, toughness) in [("Crimson Acolyte", 1, 1), ("True Believer", 2, 2)] {
        let card = def(name);
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        assert!(card.subtypes.contains(&Subtype::Human), "{name}");
        assert!(card.subtypes.contains(&Subtype::Cleric), "{name}");
    }
    assert_eq!(def("Crimson Acolyte").activated_abilities.len(), 1);
    assert!(def("Cursed Totem").has_type(CardType::Artifact));
}

// ---- Mass removal ----------------------------------------------------------------

#[test]
fn anarchy_destroys_every_permanent_that_is_white_now() {
    let mut state = islands();
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let sphere = put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    let acolyte = put(&mut state, P0, "Crimson Acolyte", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let plains = put(&mut state, P1, "Plains", Zone::Battlefield);
    let anarchy = put(&mut state, P0, "Anarchy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, anarchy, &[]);
    settled(&mut state);
    for white in [mother, sphere, acolyte] {
        assert_eq!(zone(&state, white), Zone::Graveyard);
    }
    assert_eq!(zone(&state, specter), Zone::Battlefield, "a black creature");
    assert_eq!(
        zone(&state, plains),
        Zone::Battlefield,
        "a land is colorless"
    );
    assert_eq!(zone(&state, anarchy), Zone::Graveyard);

    // Painter's Servant naming white makes every permanent white.
    let mut state = islands();
    let painter = put(&mut state, P1, "Painter's Servant", Zone::Battlefield);
    state.objects.get_mut(painter).v4.chosen_color = Some(ManaColor::W);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let anarchy = put(&mut state, P0, "Anarchy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, anarchy, &[]);
    settled(&mut state);
    for painted in [painter, specter, island] {
        assert_eq!(zone(&state, painted), Zone::Graveyard);
    }
}

#[test]
fn tranquil_domain_destroys_every_non_aura_enchantment() {
    let mut state = islands();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let flare = put(&mut state, P0, "Essence Flare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, flare, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(zone(&state, flare), Zone::Battlefield);
    let sphere = put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    let opalescence = put(&mut state, P1, "Opalescence", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    let totem = put(&mut state, P1, "Cursed Totem", Zone::Battlefield);
    let domain = put(&mut state, P0, "Tranquil Domain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, domain, &[]);
    settled(&mut state);
    for enchantment in [sphere, opalescence, seal] {
        assert_eq!(zone(&state, enchantment), Zone::Graveyard);
    }
    assert_eq!(zone(&state, flare), Zone::Battlefield, "an Aura stays");
    assert_eq!(zone(&state, totem), Zone::Battlefield, "an artifact stays");
    assert_eq!(zone(&state, domain), Zone::Graveyard);
}

#[test]
fn overload_destroys_by_mana_value_two_or_five_if_kicked() {
    for (name, kicked, destroyed) in [
        ("Sol Ring", false, true),
        ("Grim Monolith", false, true),
        ("Masticore", false, false),
        ("Masticore", true, true),
        ("Wurmcoil Engine", true, false),
    ] {
        let mut state = islands();
        let artifact = put(&mut state, P1, name, Zone::Battlefield);
        let overload = put(&mut state, P0, "Overload", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], if kicked { 2 } else { 0 });
        act(&mut state, Action::CastSpell(overload));
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseKicker { .. } => Action::ChooseKicker(kicked),
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(artifact)));
                Action::ChooseTarget(Target::Object(artifact))
            }
            other => first_answer(other),
        });
        assert_eq!(state.players[0].mana_pool, [0; 6], "{name}: paid");
        assert_eq!(
            zone(&state, artifact) == Zone::Graveyard,
            destroyed,
            "{name}, kicked {kicked}"
        );
        assert_eq!(zone(&state, overload), Zone::Graveyard);
    }
}

// ---- Spell-cast triggers -------------------------------------------------------------

#[test]
fn pyrostatic_pillar_damages_each_player_who_casts_a_cheap_spell_both_seats() {
    for caster in [P0, P1] {
        // Mana value 2: its caster takes 2, whoever controls the Pillar.
        let mut state = islands();
        put(&mut state, P1, "Pyrostatic Pillar", Zone::Battlefield);
        state.priority_player = caster;
        let incinerate = put(&mut state, caster, "Incinerate", Zone::Hand);
        pool(&mut state, caster, &[ManaColor::R], 1);
        cast(&mut state, incinerate, &[Target::Player(caster.opponent())]);
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let mut copy = restored(&state);
        for continued in [&mut state, &mut copy] {
            settled(continued);
            assert_eq!(life(continued, caster), 18, "the Pillar's 2");
            assert_eq!(life(continued, caster.opponent()), 17, "Incinerate's 3");
        }
    }

    // Mana value 4 does not trigger it.
    let mut state = islands();
    put(&mut state, P1, "Pyrostatic Pillar", Zone::Battlefield);
    let anarchy = put(&mut state, P0, "Anarchy", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::R], 2);
    cast(&mut state, anarchy, &[]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 20);

    // The announced X counts: Earthquake for 2 has mana value 3, for 3 has 4.
    for (x, own_life) in [(2, 16), (3, 17)] {
        let mut state = islands();
        put(&mut state, P1, "Pyrostatic Pillar", Zone::Battlefield);
        let quake = put(&mut state, P0, "Earthquake", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], x as u8);
        act(&mut state, Action::CastSpell(quake));
        announce_x(&mut state, x);
        settled(&mut state);
        assert_eq!(life(&state, P0), own_life, "X = {x}");
        assert_eq!(life(&state, P1), 20 - i32::from(x));
    }

    // A split half has its own mana value: Ice's is 2.
    let mut state = islands();
    let pillar = put(&mut state, P1, "Pyrostatic Pillar", Zone::Battlefield);
    let ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_half(&mut state, ice, 1, &[Target::Object(pillar)]);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn pillar_damage_survives_the_pillar_leaving() {
    let mut state = islands();
    let pillar = put(&mut state, P1, "Pyrostatic Pillar", Zone::Battlefield);
    let incinerate = put(&mut state, P0, "Incinerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, incinerate, &[Target::Player(P1)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 2, "the trigger is above the spell");
    assert_eq!(state.stack.last().unwrap().controller, P1);
    bury(&mut state, pillar);
    settled(&mut state);
    assert_eq!(life(&state, P0), 18);
}

#[test]
fn warmth_gains_two_life_when_an_opponent_casts_a_red_spell() {
    let mut state = islands();
    put(&mut state, P1, "Warmth", Zone::Battlefield);
    let incinerate = put(&mut state, P0, "Incinerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, incinerate, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19, "20 + 2 - 3");

    // A blue half and a green spell are not red.
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, mongrel, &[]);
    settled(&mut state);
    let ice = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_half(&mut state, ice, 1, &[Target::Object(mongrel)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19);

    // Its controller's own red spell does not trigger it.
    state.priority_player = P1;
    let own = put(&mut state, P1, "Incinerate", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    cast(&mut state, own, &[Target::Player(P0)]);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19);
    assert_eq!(life(&state, P0), 17);
}

// ---- Statics ------------------------------------------------------------------------

#[test]
fn chill_taxes_red_spells_by_the_cast_forms_color() {
    let mut state = islands();
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Hand);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R, ManaColor::G], 0);
    assert!(!castable(&mut state, fanatic), "{{R}} and {{2}} more");
    assert!(castable(&mut state, mongrel), "a green spell is not taxed");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, fanatic, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6], "paid {{2}}{{R}}");

    // Each Chill adds {2}.
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let second = put(&mut state, P0, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 3);
    assert!(!castable(&mut state, second));
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, second));

    // The cast half's color: Fire is red, Ice is blue.
    let mut state = islands();
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let split = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, split), "Fire costs {{3}}{{R}}");
    let mut state = islands();
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let split = put(&mut state, P0, "Fire // Ice", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast_half(&mut state, split, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(state.objects.get(elves).tapped, "Ice for {{1}}{{U}}");

    // Painter's Servant naming red makes every spell red.
    let mut state = islands();
    put(&mut state, P1, "Chill", Zone::Battlefield);
    let painter = put(&mut state, P1, "Painter's Servant", Zone::Battlefield);
    state.objects.get_mut(painter).v4.chosen_color = Some(ManaColor::R);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    assert!(!castable(&mut state, mongrel), "a painted spell is red");
}

#[test]
fn cursed_totem_stops_activated_abilities_of_creatures_on_the_battlefield() {
    // A creature's mana ability can't pay for a spell.
    let mut state = islands();
    let totem = put(&mut state, P1, "Cursed Totem", Zone::Battlefield);
    put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    assert!(!castable(&mut state, ring));
    bury(&mut state, totem);
    assert!(castable(&mut state, ring), "without the Totem");

    // Creatures' other abilities are stopped; an artifact's and a creature
    // card's cycling from hand are not.
    let mut state = islands();
    put(&mut state, P1, "Cursed Totem", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    let sharpshooter = put(&mut state, P0, "Goblin Sharpshooter", Zone::Battlefield);
    let scroll = put(&mut state, P0, "Cursed Scroll", Zone::Battlefield);
    let tusker = put(&mut state, P0, "Krosan Tusker", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 3);
    assert!(!has_activatable_ability(&mut state, mother));
    assert!(!has_activatable_ability(&mut state, sharpshooter));
    assert!(has_activatable_ability(&mut state, scroll));
    assert!(has_activatable_ability(&mut state, tusker));

    // An animated land is a creature while animated.
    let mut state = islands();
    put(&mut state, P1, "Cursed Totem", Zone::Battlefield);
    let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    assert!(castable(&mut state, ring), "the land's mana");
    assert!(has_activatable_ability(&mut state, factory));
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(factory, 0));
    settled(&mut state);
    assert!(engine::object_has_type(&state, factory, CardType::Creature));
    assert!(!state.objects.get(factory).tapped, "paid from the pool");
    assert!(!has_activatable_ability(&mut state, factory));
    assert!(!castable(&mut state, ring), "the animated land's mana");
}

#[test]
fn crimson_acolyte_has_and_grants_protection_from_red() {
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Crimson Acolyte", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let scroll = put(&mut state, P1, "Cursed Scroll", Zone::Battlefield);
    hit(&mut state, fanatic, Target::Object(acolyte), 1);
    assert_eq!(state.objects.get(acolyte).damage, 0, "red damage");
    hit(&mut state, scroll, Target::Object(acolyte), 1);
    assert_eq!(state.objects.get(acolyte).damage, 1, "colorless damage");

    // {W}: any creature, an opponent's too, until end of turn.
    let mut state = islands();
    let acolyte = put(&mut state, P0, "Crimson Acolyte", Zone::Battlefield);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::W], 0);
    act(&mut state, Action::ActivateAbility(acolyte, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            for creature in [acolyte, mongrel, fanatic] {
                assert!(legal_targets.contains(&Target::Object(creature)));
            }
        }
        other => panic!("{other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Object(mongrel)));
    settled(&mut state);
    hit(&mut state, fanatic, Target::Object(mongrel), 1);
    assert_eq!(state.objects.get(mongrel).damage, 0, "protected");
    state.priority_player = P1;
    let incinerate = put(&mut state, P1, "Incinerate", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    let targets = cast_targets(&state, incinerate);
    assert!(!targets.contains(&Target::Object(mongrel)));
    assert!(!targets.contains(&Target::Object(acolyte)));
    assert!(targets.contains(&Target::Object(fanatic)));
    state.priority_player = P0;
    to_main_phase_of(&mut state, P0);
    hit(&mut state, fanatic, Target::Object(mongrel), 1);
    assert_eq!(state.objects.get(mongrel).damage, 1, "the turn ended");
}

#[test]
fn true_believer_gives_only_shroud_to_its_controller() {
    let mut state = islands();
    let believer = put(&mut state, P1, "True Believer", Zone::Battlefield);
    let incinerate = put(&mut state, P0, "Incinerate", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    let targets = cast_targets(&state, incinerate);
    assert!(!targets.contains(&Target::Player(P1)));
    assert!(targets.contains(&Target::Player(P0)));
    assert!(targets.contains(&Target::Object(believer)), "the creature");
    state.priority_player = P1;
    let own = put(&mut state, P1, "Incinerate", Zone::Hand);
    pool(&mut state, P1, &[ManaColor::R], 1);
    assert!(
        !cast_targets(&state, own).contains(&Target::Player(P1)),
        "its controller can't target themselves"
    );
    state.priority_player = P0;

    // Damage that doesn't target is dealt, and the draw step is not skipped.
    let quake = put(&mut state, P0, "Earthquake", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    act(&mut state, Action::CastSpell(quake));
    announce_x(&mut state, 1);
    settled(&mut state);
    assert_eq!(life(&state, P1), 19);
    let hand = state.players[1].hand.len();
    to_main_phase_of(&mut state, P1);
    assert_eq!(state.players[1].hand.len(), hand + 1);
}

#[test]
fn sphere_of_law_prevents_two_of_each_red_sources_damage_to_its_controller() {
    let mut state = islands();
    put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Battlefield);
    let scroll = put(&mut state, P0, "Cursed Scroll", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    hit(&mut state, fanatic, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 19, "2 of 3 prevented");
    hit(&mut state, fanatic, Target::Player(P1), 2);
    assert_eq!(life(&state, P1), 19, "all of 2");
    hit(&mut state, scroll, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 16, "a colorless source");
    hit(&mut state, fanatic, Target::Object(specter), 1);
    assert_eq!(state.objects.get(specter).damage, 1, "a creature");
    hit(&mut state, fanatic, Target::Player(P0), 3);
    assert_eq!(life(&state, P0), 17, "the other player");

    // Each Sphere prevents 2.
    put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    hit(&mut state, fanatic, Target::Player(P1), 5);
    assert_eq!(life(&state, P1), 15);

    // Damage that can't be prevented is dealt in full.
    let pain = put(&mut state, P0, "Flaring Pain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    cast(&mut state, pain, &[]);
    settled(&mut state);
    hit(&mut state, fanatic, Target::Player(P1), 3);
    assert_eq!(life(&state, P1), 12);
}

// ---- Essence Flare -------------------------------------------------------------------

#[test]
fn essence_flare_boosts_and_shrinks_the_enchanted_creature_each_upkeep() {
    let mut state = islands();
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let flare = put(&mut state, P0, "Essence Flare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, flare, &[Target::Object(mongrel)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, mongrel), (4, 2));
    // Only the enchanted creature's controller's upkeep.
    to_main_phase_of(&mut state, P1);
    assert_eq!(state.objects.get(mongrel).counters.minus0_minus1, 0);
    to_main_phase_of(&mut state, P0);
    assert_eq!(state.objects.get(mongrel).counters.minus0_minus1, 1);
    assert_eq!(power_toughness(&state, mongrel), (4, 1));
    to_main_phase_of(&mut state, P0);
    assert_eq!(zone(&state, mongrel), Zone::Graveyard, "4/0");
    assert_eq!(zone(&state, flare), Zone::Graveyard);
}

#[test]
fn essence_flare_on_an_opponents_creature_is_its_controllers_trigger() {
    let mut state = islands();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let flare = put(&mut state, P0, "Essence Flare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, flare, &[Target::Object(specter)]);
    settled(&mut state);
    assert_eq!(power_toughness(&state, specter), (4, 2));
    to_next_upkeep_trigger(&mut state);
    assert_eq!(state.active_player, P1);
    let trigger = state.stack.last().unwrap();
    assert_eq!(trigger.source, flare);
    assert_eq!(trigger.controller, P0, "the Aura's controller");
    let mut copy = restored(&state);
    assert_eq!(next(&mut state), next(&mut copy));
    for continued in [&mut state, &mut copy] {
        pass_until(continued, |state| state.step == Step::Main1);
        assert_eq!(continued.objects.get(specter).counters.minus0_minus1, 1);
        assert_eq!(power_toughness(continued, specter), (4, 1));
    }

    // The counter goes on that creature even if the Aura has left.
    let mut state = islands();
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let flare = put(&mut state, P0, "Essence Flare", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, flare, &[Target::Object(specter)]);
    settled(&mut state);
    to_next_upkeep_trigger(&mut state);
    bury(&mut state, flare);
    pass_until(&mut state, |state| state.step == Step::Main1);
    assert_eq!(state.objects.get(specter).counters.minus0_minus1, 1);
    assert_eq!(power_toughness(&state, specter), (2, 1));
}
