//! Premodern sideboard wave P3b: native rules tests for the cards that
//! complete the 75s of Terrageddon, Mono-Black Aggro, Survival, The Rock and
//! Mono-Red Control.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_premodern_p3b_v1::{read_state, statics};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const P3B_CARDS: [&str; 10] = [
    "Dystopia",
    "Engineered Plague",
    "Gloom",
    "Goblin Tinkerer",
    "Haunting Echoes",
    "Mogg Salvage",
    "Plaguebearer",
    "Ray of Revelation",
    "Xantid Swarm",
    "Zuran Orb",
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
        0x5033_4200_0001,
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

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

/// The age counters on each Dystopia.
fn age_counters(state: &GameState) -> Vec<u16> {
    read_state(state).map_or_else(Vec::new, |p3b| {
        p3b.age_counters.iter().map(|(_, count)| *count).collect()
    })
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

/// The first legal answer to a choice: the first target or option,
/// finishing a selection that may finish.
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

/// The targets `source`'s ability `index` may choose, activated now.
fn ability_targets(state: &GameState, source: ObjectId, index: u8) -> Vec<Target> {
    let mut probe = state.clone();
    assert!(activatable(&mut probe, source, index));
    act(&mut probe, Action::ActivateAbility(source, index));
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

/// Passes priority to the active player's attack declaration and attacks
/// with `attackers`.
fn attack_with(state: &mut GameState, attackers: &[ObjectId]) {
    for _ in 0..20 {
        match next(state) {
            Decision::DeclareAttackers { eligible, .. } => {
                for attacker in attackers {
                    assert!(eligible.contains(attacker), "{attacker:?} can't attack");
                }
                act(state, Action::DeclareAttackers(attackers.to_vec()));
                return;
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("expected the attack declaration, got {other:?}"),
        }
    }
    panic!("no attack declaration");
}

/// Moves `object` to its owner's graveyard, as if it were destroyed.
fn bury(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn p3b_cards_are_full_with_their_printed_characteristics() {
    for name in P3B_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
        assert!(!def(name).is_token, "{name}");
    }
    for (name, power, toughness, subtypes) in [
        (
            "Goblin Tinkerer",
            1,
            2,
            &[Subtype::Goblin, Subtype::Artificer][..],
        ),
        ("Plaguebearer", 1, 1, &[Subtype::Zombie][..]),
        ("Xantid Swarm", 0, 1, &[Subtype::Insect][..]),
    ] {
        let card = def(name);
        assert_eq!(
            (card.power, card.toughness),
            (Some(power), Some(toughness)),
            "{name}"
        );
        for subtype in subtypes {
            assert!(card.subtypes.contains(subtype), "{name}: {subtype:?}");
        }
    }
    assert!(def("Xantid Swarm").keywords.has(Keywords::FLYING));
    assert_eq!(
        def("Plaguebearer").activated_abilities.len(),
        9,
        "one for each X from 0 to 8"
    );
    assert_eq!(def("Goblin Tinkerer").activated_abilities.len(), 1);
    assert_eq!(def("Zuran Orb").activated_abilities.len(), 1);
    assert!(def("Ray of Revelation").flashback.is_some());
    assert!(def("Mogg Salvage").alt_cost.is_some());
    for name in ["Dystopia", "Engineered Plague", "Gloom"] {
        assert!(def(name).has_type(CardType::Enchantment), "{name}");
    }
    assert!(def("Zuran Orb").has_type(CardType::Artifact));
}

// ---- Activated abilities ------------------------------------------------------------

#[test]
fn goblin_tinkerer_destroys_an_artifact_that_deals_its_mana_value_back() {
    for (name, survives, damage) in [
        ("Tormod's Crypt", true, 0),
        ("Sol Ring", true, 1),
        ("Masticore", false, 0),
    ] {
        let mut state = islands();
        let tinkerer = put(&mut state, P0, "Goblin Tinkerer", Zone::Battlefield);
        let artifact = put(&mut state, P1, name, Zone::Battlefield);
        let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
        pool(&mut state, P0, &[ManaColor::R], 0);
        let targets = ability_targets(&state, tinkerer, 0);
        assert!(targets.contains(&Target::Object(artifact)), "{name}");
        assert!(
            !targets.contains(&Target::Object(mongrel)),
            "not an artifact"
        );
        activate(&mut state, tinkerer, 0, &[Target::Object(artifact)]);
        settled(&mut state);
        assert_eq!(zone(&state, artifact), Zone::Graveyard, "{name}");
        if survives {
            assert_eq!(zone(&state, tinkerer), Zone::Battlefield, "{name}");
            assert_eq!(state.objects.get(tinkerer).damage, damage, "{name}");
            assert!(state.objects.get(tinkerer).tapped);
        } else {
            assert_eq!(zone(&state, tinkerer), Zone::Graveyard, "{name}: 4 damage");
        }
    }

    // The artifact is destroyed even if the Tinkerer has left.
    let mut state = islands();
    let tinkerer = put(&mut state, P0, "Goblin Tinkerer", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::R], 0);
    activate(&mut state, tinkerer, 0, &[Target::Object(ring)]);
    bury(&mut state, tinkerer);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
}

#[test]
fn plaguebearer_destroys_a_nonblack_creature_with_mana_value_x() {
    let mut state = islands();
    let plaguebearer = put(&mut state, P0, "Plaguebearer", Zone::Battlefield);
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let rats = put(&mut state, P1, "Ravenous Rats", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    // X = 3 costs {6}{B}, and the only 3-drop is black.
    pool(&mut state, P0, &[ManaColor::B], 6);
    assert!(!activatable(&mut state, plaguebearer, 3));
    // X = 2 costs {4}{B}: the Mongrel, not the black Rats or Plaguebearer.
    assert_eq!(
        ability_targets(&state, plaguebearer, 2),
        vec![Target::Object(mongrel)]
    );
    activate(&mut state, plaguebearer, 2, &[Target::Object(mongrel)]);
    settled(&mut state);
    assert_eq!(zone(&state, mongrel), Zone::Graveyard);
    assert_eq!(
        state.players[0].mana_pool[ManaColor::C.pool_index()],
        2,
        "paid {{4}}{{B}}"
    );
    // X = 1 costs {2}{B}.
    assert!(
        !activatable(&mut state, plaguebearer, 1),
        "no black mana left"
    );
    pool(&mut state, P0, &[ManaColor::B], 0);
    activate(&mut state, plaguebearer, 1, &[Target::Object(mother)]);
    settled(&mut state);
    assert_eq!(zone(&state, mother), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    for kept in [rats, specter, plaguebearer] {
        assert_eq!(zone(&state, kept), Zone::Battlefield);
    }

    // The target is checked again: a creature that turned black fizzles it.
    let mut state = islands();
    let plaguebearer = put(&mut state, P0, "Plaguebearer", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    pool(&mut state, P0, &[ManaColor::B], 4);
    activate(&mut state, plaguebearer, 2, &[Target::Object(mongrel)]);
    let painter = put(&mut state, P1, "Painter's Servant", Zone::Battlefield);
    state.objects.get_mut(painter).v4.chosen_color = Some(ManaColor::B);
    settled(&mut state);
    assert_eq!(zone(&state, mongrel), Zone::Battlefield);
}

#[test]
fn zuran_orb_sacrifices_a_land_for_two_life() {
    let mut state = islands();
    let orb = put(&mut state, P0, "Zuran Orb", Zone::Battlefield);
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let wasteland = put(&mut state, P0, "Wasteland", Zone::Battlefield);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Battlefield);
    let opposing = put(&mut state, P1, "Island", Zone::Battlefield);
    for land in [wasteland, island] {
        activate(&mut state, orb, 0, &[]);
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&land), "{candidates:?}");
                for other in [orb, ring, opposing] {
                    assert!(!candidates.contains(&other), "{candidates:?}");
                }
                Action::ChooseCostTarget(land)
            }
            other => panic!("unexpected choice: {other:?}"),
        });
        assert_eq!(zone(&state, land), Zone::Graveyard);
    }
    assert_eq!(life(&state, P0), 24);
    assert!(!activatable(&mut state, orb, 0), "no land left");
    assert_eq!(zone(&state, opposing), Zone::Battlefield);
}

// ---- Triggered abilities ------------------------------------------------------------

#[test]
fn xantid_swarm_attacking_stops_the_defending_players_spells_this_turn() {
    let mut state = islands();
    let swarm = put(&mut state, P0, "Xantid Swarm", Zone::Battlefield);
    let incinerate = put(&mut state, P1, "Incinerate", Zone::Hand);
    let own = put(&mut state, P0, "Incinerate", Zone::Hand);
    attack_with(&mut state, &[swarm]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1, "the attack trigger");
    let mut copy = restored(&state);
    for continued in [&mut state, &mut copy] {
        settled(continued);
        pool(continued, P0, &[ManaColor::R], 1);
        pool(continued, P1, &[ManaColor::R], 1);
        assert!(castable(continued, own), "the attacking player's spells");
        act(continued, Action::Pass);
        assert_eq!(continued.priority_player, P1);
        assert!(!castable(continued, incinerate), "the defending player's");
    }
    assert_eq!(state.state_hash(), copy.state_hash());

    // The ban ends with the turn.
    to_main_phase_of(&mut state, P1);
    pool(&mut state, P1, &[ManaColor::R], 1);
    assert!(castable(&mut state, incinerate));
}

#[test]
fn dystopia_takes_cumulative_life_and_a_green_or_white_permanent_each_upkeep() {
    let mut state = islands();
    let dystopia = put(&mut state, P0, "Dystopia", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let forest = put(&mut state, P0, "Forest", Zone::Battlefield);
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let specter = put(&mut state, P1, "Hypnotic Specter", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);

    // P1's upkeep: P1 sacrifices a green or white permanent of their choice.
    // Dystopia's controller controls the trigger, and P1's upkeep adds no
    // age counter.
    to_upkeep_trigger_of(&mut state, P1);
    assert_eq!(state.stack.len(), 1);
    assert_eq!(state.stack[0].controller, P0);
    let mut offered = Vec::new();
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player,
            legal_targets,
            ..
        } => {
            assert_eq!(player, P1);
            offered = legal_targets;
            Action::ChooseEffectTarget(Target::Object(mother))
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(offered.len(), 2, "{offered:?}");
    assert!(offered.contains(&Target::Object(mongrel)));
    assert_eq!(zone(&state, mother), Zone::Graveyard);
    for kept in [mongrel, specter, ring, elves] {
        assert_eq!(zone(&state, kept), Zone::Battlefield);
    }
    assert!(age_counters(&state).is_empty());

    // P0's upkeep: an age counter and 1 life, and P0's own green permanent.
    to_upkeep_trigger_of(&mut state, P0);
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        settle_with(continued, |_, decision| match decision {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!((player, option_count), (P0, 2));
                Action::ChooseEffectOption(0)
            }
            other => panic!("unexpected choice: {other:?}"),
        });
        assert_eq!(zone(continued, elves), Zone::Graveyard);
        assert_eq!(
            zone(continued, forest),
            Zone::Battlefield,
            "lands are colorless"
        );
        assert_eq!(life(continued, P0), 19);
        assert_eq!(age_counters(continued), vec![1]);
    }
    assert_eq!(state.state_hash(), copy.state_hash());

    // P1's next upkeep takes the Mongrel, the only one left.
    to_upkeep_trigger_of(&mut state, P1);
    settled(&mut state);
    assert_eq!(zone(&state, mongrel), Zone::Graveyard);

    // Two counters: pay 2 life. A player who can't pay sacrifices it.
    let mut poor = restored(&state);
    poor.players[0].life = 1;
    to_upkeep_trigger_of(&mut poor, P0);
    settled(&mut poor);
    assert_eq!(zone(&poor, dystopia), Zone::Graveyard);
    assert_eq!(life(&poor, P0), 1);
    to_upkeep_trigger_of(&mut state, P0);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(0),
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(life(&state, P0), 17);
    assert_eq!(age_counters(&state), vec![2]);

    // Three counters: decline and sacrifice it.
    to_upkeep_trigger_of(&mut state, P0);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(1),
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(zone(&state, dystopia), Zone::Graveyard);
    assert_eq!(life(&state, P0), 17);
    to_main_phase_of(&mut state, P1);
    assert!(age_counters(&state).is_empty(), "pruned");
}

// ---- Statics ------------------------------------------------------------------------

#[test]
fn engineered_plague_shrinks_every_creature_of_the_chosen_type() {
    let options = statics::plague_type_options();
    assert!(options.iter().all(|subtype| subtype.is_creature_type()));
    for (index, subtype) in options.iter().enumerate() {
        assert!(
            options[..index]
                .iter()
                .all(|earlier| !earlier.same_subtype_as(*subtype)),
            "{subtype:?} twice"
        );
    }
    let goblin = options
        .iter()
        .position(|subtype| subtype.same_subtype_as(Subtype::Goblin))
        .unwrap();

    let mut state = islands();
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Battlefield);
    let tinkerer = put(&mut state, P0, "Goblin Tinkerer", Zone::Battlefield);
    let mongrel = put(&mut state, P1, "Wild Mongrel", Zone::Battlefield);
    let plague = put(&mut state, P0, "Engineered Plague", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, plague, &[]);
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption {
            player,
            source,
            option_count,
        }) => {
            assert_eq!((player, source), (P0, plague));
            assert_eq!(usize::from(option_count), options.len());
        }
        other => panic!("expected the creature type choice, got {other:?}"),
    }
    let mut copy = restored(&state);
    for continued in [&mut state, &mut copy] {
        act(continued, Action::ChooseEffectOption(goblin as u16));
        settled(continued);
        assert_eq!(
            statics::plague_chosen_type(continued, plague),
            Some(options[goblin])
        );
        assert_eq!(zone(continued, fanatic), Zone::Graveyard, "a 0/0 Goblin");
        assert_eq!(
            power_toughness(continued, tinkerer),
            (0, 1),
            "its controller's Goblin too"
        );
        assert_eq!(power_toughness(continued, mongrel), (2, 2), "a Dog");
    }
    assert_eq!(state.state_hash(), copy.state_hash());

    // A Goblin that enters later is shrunk too, and a second Plague naming
    // Goblin adds another -1/-1.
    let lackey = put(&mut state, P1, "Goblin Lackey", Zone::Battlefield);
    assert_eq!(power_toughness(&state, lackey), (0, 0));
    let second = put(&mut state, P0, "Engineered Plague", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 2);
    cast(&mut state, second, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectOption { source, .. } => {
            assert_eq!(source, second);
            Action::ChooseEffectOption(goblin as u16)
        }
        other => panic!("unexpected choice: {other:?}"),
    });
    assert_eq!(zone(&state, lackey), Zone::Graveyard);
    assert_eq!(zone(&state, tinkerer), Zone::Graveyard, "-2/-2");

    // Once the Plague leaves, its creatures recover.
    let mut state = copy;
    bury(&mut state, plague);
    assert_eq!(power_toughness(&state, tinkerer), (1, 2));
    assert_eq!(statics::plague_chosen_type(&state, plague), None);
}

#[test]
fn gloom_taxes_white_spells_and_abilities_of_white_enchantments() {
    let mut state = islands();
    put(&mut state, P1, "Gloom", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Hand);
    let fanatic = put(&mut state, P0, "Mogg Fanatic", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 2);
    assert!(!castable(&mut state, mother), "{{3}}{{W}}");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, mother, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].mana_pool, [0; 6], "paid {{3}}{{W}}");
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(castable(&mut state, fanatic), "a red spell is not taxed");

    // Abilities of white enchantments: Seal of Cleansing's sacrifice costs
    // {3}, Circle of Protection: Blue's {1} costs {4}. A red enchantment's
    // and a white creature's are not taxed.
    let mut state = islands();
    let gloom = put(&mut state, P1, "Gloom", Zone::Battlefield);
    let cleansing = put(&mut state, P0, "Seal of Cleansing", Zone::Battlefield);
    let circle = put(
        &mut state,
        P0,
        "Circle of Protection: Blue",
        Zone::Battlefield,
    );
    let seal = put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    let mother = put(&mut state, P0, "Mother of Runes", Zone::Battlefield);
    assert!(activatable(&mut state, seal, 0));
    assert!(activatable(&mut state, mother, 0));
    assert!(!activatable(&mut state, cleansing, 0));
    pool(&mut state, P0, &[], 3);
    assert!(!activatable(&mut state, circle, 0), "{{4}}");
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, circle, 0));
    activate(&mut state, cleansing, 0, &[Target::Object(gloom)]);
    settled(&mut state);
    assert_eq!(zone(&state, gloom), Zone::Graveyard);
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 1);
    assert!(activatable(&mut state, circle, 0), "{{1}} without Gloom");

    // Painter's Servant naming white makes every enchantment white.
    let mut state = islands();
    put(&mut state, P1, "Gloom", Zone::Battlefield);
    let painter = put(&mut state, P1, "Painter's Servant", Zone::Battlefield);
    state.objects.get_mut(painter).v4.chosen_color = Some(ManaColor::W);
    let seal = put(&mut state, P0, "Seal of Fire", Zone::Battlefield);
    assert!(!activatable(&mut state, seal, 0));
}

// ---- Spells -------------------------------------------------------------------------

#[test]
fn haunting_echoes_exiles_a_graveyard_but_basics_and_every_library_copy() {
    let mut state = islands();
    let basic = put(&mut state, P1, "Island", Zone::Graveyard);
    let fanatic = put(&mut state, P1, "Mogg Fanatic", Zone::Graveyard);
    let wasteland = put(&mut state, P1, "Wasteland", Zone::Graveyard);
    let copies = [
        put(&mut state, P1, "Mogg Fanatic", Zone::Library),
        put(&mut state, P1, "Mogg Fanatic", Zone::Library),
        put(&mut state, P1, "Wasteland", Zone::Library),
    ];
    let mother = put(&mut state, P1, "Mother of Runes", Zone::Library);
    let own = put(&mut state, P0, "Mogg Fanatic", Zone::Library);
    let echoes = put(&mut state, P0, "Haunting Echoes", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B, ManaColor::B], 3);
    cast(&mut state, echoes, &[Target::Player(P1)]);
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    let mut copy = restored(&state);
    for continued in [&mut state, &mut copy] {
        settled(continued);
        for exiled in [fanatic, wasteland].into_iter().chain(copies) {
            assert_eq!(zone(continued, exiled), Zone::Exile);
        }
        assert_eq!(
            zone(continued, basic),
            Zone::Graveyard,
            "a basic land card stays"
        );
        assert_eq!(zone(continued, mother), Zone::Library, "no copy exiled");
        assert_eq!(zone(continued, own), Zone::Library, "the other library");
        assert_eq!(
            continued.players[1].library.len(),
            21,
            "the Islands and the Mother"
        );
        assert_eq!(zone(continued, echoes), Zone::Graveyard);
    }
    assert_eq!(state.state_hash(), copy.state_hash());
}

#[test]
fn mogg_salvage_is_free_with_an_opposing_island_and_your_mountain() {
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let salvage = put(&mut state, P0, "Mogg Salvage", Zone::Hand);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    assert!(!castable(&mut state, salvage), "no opposing Island");
    put(&mut state, P1, "Island", Zone::Battlefield);
    cast(&mut state, salvage, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);
    assert_eq!(zone(&state, salvage), Zone::Graveyard);

    // Dual lands count by their subtypes, and cost increases still apply.
    let mut state = islands();
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let salvage = put(&mut state, P0, "Mogg Salvage", Zone::Hand);
    put(&mut state, P0, "Volcanic Island", Zone::Battlefield);
    put(&mut state, P1, "Volcanic Island", Zone::Battlefield);
    put(&mut state, P1, "Chill", Zone::Battlefield);
    assert!(!castable(&mut state, salvage), "Chill's {{2}}");
    pool(&mut state, P0, &[], 1);
    cast(&mut state, salvage, &[Target::Object(ring)]);
    settled(&mut state);
    assert_eq!(zone(&state, ring), Zone::Graveyard);

    // Without the condition, its mana cost.
    let mut state = islands();
    put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let salvage = put(&mut state, P0, "Mogg Salvage", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    assert!(!castable(&mut state, salvage));
    pool(&mut state, P0, &[], 1);
    assert!(castable(&mut state, salvage));
}

#[test]
fn ray_of_revelation_destroys_an_enchantment_and_flashes_back_for_green() {
    let mut state = islands();
    let sphere = put(&mut state, P1, "Sphere of Law", Zone::Battlefield);
    let seal = put(&mut state, P1, "Seal of Fire", Zone::Battlefield);
    let ring = put(&mut state, P1, "Sol Ring", Zone::Battlefield);
    let ray = put(&mut state, P0, "Ray of Revelation", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    let targets = cast_targets(&state, ray);
    assert!(targets.contains(&Target::Object(seal)));
    assert!(!targets.contains(&Target::Object(ring)));
    cast(&mut state, ray, &[Target::Object(sphere)]);
    settled(&mut state);
    assert_eq!(zone(&state, sphere), Zone::Graveyard);
    assert_eq!(zone(&state, ray), Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 0);
    assert!(!castable(&mut state, ray), "flashback costs {{G}}");
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, ray, &[Target::Object(seal)]);
    settled(&mut state);
    assert_eq!(zone(&state, seal), Zone::Graveyard);
    assert_eq!(zone(&state, ray), Zone::Exile);
}
