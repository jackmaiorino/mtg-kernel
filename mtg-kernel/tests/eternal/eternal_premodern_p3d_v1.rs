//! Premodern sideboard wave P3d: native rules tests for the cards that
//! complete Enchantress's 75.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CardDef, CardType, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3D_CARDS: [&str; 3] = ["Abeyance", "Karmic Justice", "Sacred Ground"];

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
        0x5033_4400_0001,
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

/// Settles with no player choice: a Karmic Justice trigger would ask for a
/// target and fail this.
fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
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

/// Activates `source`'s ability `index`, answering each target prompt from
/// `targets` in order.
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

/// Plays on until `stop` holds before a decision: passing priority and not
/// attacking or blocking.
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
            other => panic!("unexpected choice: {other:?}"),
        };
        act(state, action);
    }
    panic!("condition never held");
}

/// Passes to `player`'s next first main phase.
fn to_main_phase_of(state: &mut GameState, player: PlayerId) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.active_player == player
            && state.step == Step::Main1
    });
}

/// Passes to `player`'s next upkeep with a trigger on the stack.
fn to_upkeep_trigger_of(state: &mut GameState, player: PlayerId) {
    let now = (state.turn, state.active_player);
    pass_until(state, |state| {
        (state.turn, state.active_player) != now
            && state.active_player == player
            && state.step == Step::Upkeep
            && !state.stack.is_empty()
    });
}

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3d_cards_are_full_with_their_printed_characteristics() {
    for name in P3D_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    assert!(def("Abeyance").has_type(CardType::Sorcery));
    for (name, mana_value) in [("Abeyance", 2), ("Karmic Justice", 3), ("Sacred Ground", 2)] {
        assert_eq!(def(name).mana_value, mana_value, "{name}");
    }
    for name in ["Karmic Justice", "Sacred Ground"] {
        assert!(def(name).has_type(CardType::Enchantment), "{name}");
    }
}

// ---- Abeyance ------------------------------------------------------------------

#[test]
fn abeyance_stops_a_players_instants_sorceries_and_nonmana_abilities_this_turn() {
    let mut state = islands();
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let own = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let abeyance = put(&mut state, P0, "Abeyance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, abeyance, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(zone(&state, abeyance), Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 2, "Abeyance drew a card");

    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        pool(continued, P0, &[ManaColor::R], 0);
        assert!(castable(continued, own), "its caster's spells");
        act(continued, Action::Pass);
        assert_eq!(continued.priority_player, P1);
        pool(continued, P1, &[ManaColor::R], 0);
        assert!(!castable(continued, bolt), "an instant");
        assert!(!activatable(continued, seal, 0), "an ability");
        act(continued, Action::ActivateManaAbility(elves));
        assert_eq!(
            continued.players[1].mana_pool[ManaColor::G.pool_index()],
            1,
            "a mana ability"
        );
    }
    assert_eq!(state.state_hash(), copy.state_hash());

    // The restriction ends with the turn.
    to_main_phase_of(&mut state, P1);
    pool(&mut state, P1, &[ManaColor::R], 0);
    assert!(castable(&mut state, bolt));
    assert!(activatable(&mut state, seal, 0));
}

#[test]
fn abeyance_leaves_creature_spells() {
    let mut state = islands();
    put(&mut state, P1, "Island", Zone::Battlefield);
    let specter = put(&mut state, P0, "Hypnotic Specter", Zone::Hand);
    let rain = put(&mut state, P0, "Stone Rain", Zone::Hand);
    let abeyance = put(&mut state, P0, "Abeyance", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, abeyance, &[Target::Player(P0)]);
    settled(&mut state);
    pool(
        &mut state,
        P0,
        &[ManaColor::B, ManaColor::B, ManaColor::R],
        3,
    );
    assert!(castable(&mut state, specter), "a creature spell");
    assert!(!castable(&mut state, rain), "a sorcery");
}

// ---- Karmic Justice ------------------------------------------------------------

#[test]
fn karmic_justice_may_destroy_a_permanent_of_the_opponent_who_destroyed_one() {
    for destroy in [true, false] {
        let mut state = islands();
        let justice = put(&mut state, P1, "Karmic Justice", Zone::Battlefield);
        let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
        let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
        let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
        let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::W], 1);
        cast(&mut state, disenchant, &[Target::Object(seal)]);
        let decision = settle(&mut state).expect("Karmic Justice's target");
        let Decision::ChooseTargets {
            player,
            spell,
            legal_targets,
            ..
        } = decision
        else {
            panic!("expected targets, got {decision:?}");
        };
        assert_eq!((player, spell), (P1, justice));
        assert_eq!(zone(&state, seal), Zone::Graveyard);
        assert!(legal_targets.contains(&Target::Object(ring)));
        assert!(
            legal_targets.contains(&Target::Object(elves)),
            "any permanent"
        );
        assert!(!legal_targets.contains(&Target::Object(justice)), "theirs");
        act(&mut state, Action::ChooseTarget(Target::Object(ring)));

        let mut copy = restored(&state);
        assert_eq!(state.state_hash(), copy.state_hash());
        for continued in [&mut state, &mut copy] {
            settle_with(continued, |_, decision| match decision {
                Decision::ChooseEffectOption {
                    player,
                    option_count,
                    ..
                } => {
                    assert_eq!((player, option_count), (P1, 2));
                    Action::ChooseEffectOption(if destroy { 0 } else { 1 })
                }
                other => panic!("unexpected choice: {other:?}"),
            });
            let expected = if destroy {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            };
            assert_eq!(zone(continued, ring), expected, "destroy: {destroy}");
            assert_eq!(zone(continued, elves), Zone::Battlefield);
        }
        assert_eq!(state.state_hash(), copy.state_hash());
    }
}

#[test]
fn karmic_justice_destroyed_with_another_enchantment_triggers_for_both() {
    let mut state = islands();
    let justice = put(&mut state, P1, "Karmic Justice", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let monolith = put(&mut state, P0, "Grim Monolith", Zone::Battlefield);
    let domain = put(&mut state, P0, "Tranquil Domain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G], 1);
    cast(&mut state, domain, &[]);
    let mut chosen = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P1);
            let target = *legal_targets
                .iter()
                .find(|target| !chosen.contains(*target))
                .expect("an unchosen target");
            chosen.push(target);
            Action::ChooseTarget(target)
        }
        Decision::ChooseEffectOption { player, .. } => {
            assert_eq!(player, P1);
            Action::ChooseEffectOption(0)
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(chosen.len(), 2, "Karmic Justice sees its own destruction");
    for object in [justice, seal, ring, monolith] {
        assert_eq!(zone(&state, object), Zone::Graveyard);
    }
}

#[test]
fn karmic_justice_ignores_creatures_sacrifices_and_its_controllers_own_spells() {
    // A creature.
    let mut state = islands();
    put(&mut state, P1, "Karmic Justice", Zone::Battlefield);
    let masticore = put(&mut state, P1, "Masticore", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, disenchant, &[Target::Object(masticore)]);
    settled(&mut state);
    assert_eq!(zone(&state, masticore), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);

    // Its controller's own spell.
    let mut state = islands();
    put(&mut state, P0, "Karmic Justice", Zone::Battlefield);
    let seal = put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, disenchant, &[Target::Object(seal)]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);

    // A sacrifice an opponent's ability makes: Dystopia's trigger,
    // controlled by P0, has P1 sacrifice Karmic Justice itself.
    let mut state = islands();
    put(&mut state, P0, "Dystopia", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let justice = put(&mut state, P1, "Karmic Justice", Zone::Battlefield);
    put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    to_upkeep_trigger_of(&mut state, P1);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets { player, .. } => {
            assert_eq!(player, P1);
            Action::ChooseEffectTarget(Target::Object(justice))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(zone(&state, justice), Zone::Graveyard);
    assert_eq!(zone(&state, ring), Zone::Battlefield);
}

#[test]
fn karmic_justice_without_a_permanent_to_target_does_nothing() {
    let mut state = islands();
    let justice = put(&mut state, P1, "Karmic Justice", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let disenchant = put(&mut state, P0, "Disenchant", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    cast(&mut state, disenchant, &[Target::Object(seal)]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert_eq!(zone(&state, justice), Zone::Battlefield);
    assert!(state.stack.is_empty());
}

// ---- Sacred Ground -------------------------------------------------------------

#[test]
fn sacred_ground_returns_a_land_an_opponents_spell_or_ability_destroys() {
    let mut state = islands();
    put(&mut state, P1, "Sacred Ground", Zone::Battlefield);
    let tundra = put(&mut state, P1, "Tundra", Zone::Battlefield);
    let scrubland = put(&mut state, P1, "Scrubland", Zone::Battlefield);
    let wasteland = put(&mut state, P0, "Wasteland", Zone::Battlefield);
    let rain = put(&mut state, P0, "Stone Rain", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 2);
    cast(&mut state, rain, &[Target::Object(tundra)]);
    settled(&mut state);
    let returned = state.objects.get(tundra);
    assert_eq!(returned.zone, Zone::Battlefield, "Stone Rain");
    assert_eq!(returned.controller, P1);
    assert_eq!(returned.zone_change_count, 2, "a new object");

    activate(&mut state, wasteland, 0, &[Target::Object(scrubland)]);
    settled(&mut state);
    assert_eq!(zone(&state, scrubland), Zone::Battlefield, "Wasteland");
    assert_eq!(zone(&state, wasteland), Zone::Graveyard, "P0's own land");
}

#[test]
fn sacred_ground_returns_an_opponents_armageddon_lands_but_not_its_casters() {
    let mut state = islands();
    let mine = put(&mut state, P0, "Plains", Zone::Battlefield);
    put(&mut state, P0, "Sacred Ground", Zone::Battlefield);
    put(&mut state, P1, "Sacred Ground", Zone::Battlefield);
    let theirs = [
        put(&mut state, P1, "Plains", Zone::Battlefield),
        put(&mut state, P1, "Tundra", Zone::Battlefield),
        put(&mut state, P1, "Wasteland", Zone::Battlefield),
    ];
    let armageddon = put(&mut state, P0, "Armageddon", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, armageddon, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, mine), Zone::Graveyard, "P0's own spell");
    for land in theirs {
        assert_eq!(zone(&state, land), Zone::Battlefield);
        assert_eq!(state.objects.get(land).controller, P1);
    }
}

#[test]
fn sacred_ground_returns_a_land_creature_an_opponents_edict_takes() {
    let mut state = islands();
    put(&mut state, P0, "Sacred Ground", Zone::Battlefield);
    let factory = put(&mut state, P0, "Mishra's Factory", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let edict = put(&mut state, P1, "Diabolic Edict", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    activate(&mut state, factory, 0, &[]);
    settled(&mut state);
    assert!(is_creature(&state, factory));
    act(&mut state, Action::Pass);
    assert_eq!(state.priority_player, P1);
    pool(&mut state, P1, &[ManaColor::B], 1);
    cast(&mut state, edict, &[Target::Player(P0)]);
    // P0 chooses while the Edict resolves.
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P0);
            assert!(legal_targets.contains(&Target::Object(elves)));
            Action::ChooseEffectTarget(Target::Object(factory))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(zone(&state, factory), Zone::Battlefield);
    assert!(!is_creature(&state, factory), "a new object");
    assert_eq!(zone(&state, elves), Zone::Battlefield);
}
