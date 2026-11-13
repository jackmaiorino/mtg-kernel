//! Vintage sideboard wave S1f: native rules tests for Mycosynth Lattice,
//! which completes Raker Shops and Sphere Shops, and Overencumbered, which
//! completes Lurrus Breach, with the Junk token it creates.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, mana_color_mask, CardCapability, CardDef, CardType, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_sideboard_s1f_v1::read_state;
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{FaceDownV1, GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const LATTICE: &str = "Mycosynth Lattice";
const OVERENCUMBERED: &str = "Overencumbered";
const JUNK: &str = "Junk Token";
const TOKENS: [&str; 3] = ["Clue Token", "Food Token", JUNK];

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
        0x5331_4600_0001,
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

fn token(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Battlefield);
    state.objects.get_mut(object).v4.is_token = true;
    object
}

/// A permanent that entered from `player`'s hand, so it has a timestamp
/// and what entering gives it (a planeswalker's loyalty).
fn entered(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let object = put(state, player, name, Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Battlefield));
    settled(state);
    assert_eq!(zone(state, object), Zone::Battlefield);
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

fn has_type(state: &GameState, object: ObjectId, card_type: CardType) -> bool {
    engine::object_has_type(state, object, card_type)
}

fn color(state: &GameState, object: ObjectId) -> u8 {
    engine::object_color_mask(state, object)
}

/// The permanents named `name` that `player` controls.
fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    let card_def = id(name);
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).card_def == card_def)
        .collect()
}

/// The players the recorded Overencumbered Auras enchant.
fn enchanted(state: &GameState) -> Vec<PlayerId> {
    read_state(state).map_or_else(Vec::new, |s1f| {
        s1f.enchanted.iter().map(|&(_, player)| player).collect()
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

/// Settles, answering every choice with `answer`.
fn settle_with(state: &mut GameState, mut answer: impl FnMut(&GameState, Decision) -> Action) {
    while let Some(decision) = settle(state) {
        let action = answer(state, decision);
        act(state, action);
    }
}

/// Settles, answering every option choice with `option`.
fn settle_picking(state: &mut GameState, option: u16) {
    settle_with(state, |_, decision| match decision {
        Decision::ChooseEffectOption { .. } => Action::ChooseEffectOption(option),
        other => first_answer(other),
    });
}

/// The first legal answer to a choice.
fn first_answer(decision: Decision) -> Action {
    match decision {
        Decision::ChooseTargets {
            can_finish: true,
            ref legal_targets,
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
        Decision::ChooseCostTargets { candidates, .. } => Action::ChooseCostTarget(candidates[0]),
        Decision::ChooseCastMode { options, .. } => Action::ChooseCastMode(options[0]),
        Decision::ChooseKicker { .. } => Action::ChooseKicker(false),
        Decision::Discard { count, choices, .. } => {
            Action::Discard(choices.into_iter().take(count as usize).collect())
        }
        other => panic!("unexpected choice: {other:?}"),
    }
}

/// The priority holder's castable spells, activatable abilities and land
/// drops.
fn priority(state: &mut GameState) -> (Vec<ObjectId>, Vec<(ObjectId, u8)>, Vec<ObjectId>) {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells,
            activatable_abilities,
            land_drops,
            ..
        } => (castable_spells, activatable_abilities, land_drops),
        other => panic!("expected priority, got {other:?}"),
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    priority(state).0.contains(&spell)
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).1.contains(&(source, index))
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

/// Casts `spell` for its mana cost, answering each target prompt from
/// `targets` in order.
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

/// A serialized round trip of `state`.
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}

/// Plays on until `stop` holds before a decision, answering it with
/// `answer` when that gives an action and otherwise passing priority, not
/// attacking or blocking, keeping triggers in order and taking first
/// answers.
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
            other => first_answer(other),
        };
        act(state, action);
    }
    panic!("condition never held");
}

fn pass_until(state: &mut GameState, stop: impl Fn(&GameState) -> bool) {
    run_until(state, stop, |_, _| None);
}

/// P1 becomes the active player in its first main phase, holding priority.
fn opponents_main_phase(state: &mut GameState) {
    state.active_player = P1;
    state.priority_player = P1;
    state.step = Step::Main1;
}

/// `player` gains control of the permanent `object`, as a control-changing
/// effect does.
fn give_control(state: &mut GameState, object: ObjectId, player: PlayerId) {
    let previous = state.objects.get(object).controller;
    state.players[previous.index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[player.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = player;
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn s1f_cards_are_full_with_their_printed_characteristics() {
    for name in [LATTICE, OVERENCUMBERED, JUNK] {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }
    let lattice = def(LATTICE);
    assert!(!lattice.is_token);
    assert_eq!(lattice.types, &[CardType::Artifact][..]);
    assert_eq!((lattice.mana_value, lattice.colors), (6, &[][..]));
    let aura = def(OVERENCUMBERED);
    assert!(!aura.is_token);
    assert_eq!(aura.types, &[CardType::Enchantment][..]);
    assert_eq!(aura.subtypes, &[Subtype::Aura][..]);
    assert_eq!((aura.mana_value, aura.colors), (2, &[ManaColor::W][..]));
    let junk = def(JUNK);
    assert!(junk.is_token);
    assert_eq!(junk.types, &[CardType::Artifact][..]);
    assert_eq!(junk.subtypes, &[Subtype::Junk][..]);
    assert_eq!(junk.activated_abilities.len(), 1);
    assert!(junk.activated_abilities[0].sorcery_speed_only);
    assert!(!Subtype::Junk.is_creature_type());
}

// ---- Mycosynth Lattice: types ---------------------------------------------------

#[test]
fn mycosynth_lattice_makes_every_permanent_an_artifact_in_addition() {
    let mut state = islands();
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let elves = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    let goblin = token(&mut state, P1, "Goblin Token");
    let face_down = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    state.objects.get_mut(face_down).v4.face_down_v1 = Some(FaceDownV1 {
        disguised: true,
        lookers: 0,
        hidden_by: None,
        hidden_by_source: None,
    });
    let spell = put(&mut state, P0, "Llanowar Elves", Zone::Hand);
    let card = put(&mut state, P1, "Island", Zone::Graveyard);
    let permanents = [island, elves, goblin, face_down];
    for object in permanents {
        assert!(!has_type(&state, object, CardType::Artifact));
    }

    let lattice = entered(&mut state, P0, LATTICE);
    for object in permanents.into_iter().chain([lattice]) {
        assert!(
            has_type(&state, object, CardType::Artifact),
            "{}",
            state.objects.get(object).name
        );
    }
    assert!(has_type(&state, island, CardType::Land), "in addition");
    assert!(has_type(&state, elves, CardType::Creature));
    assert!(has_type(&state, face_down, CardType::Creature));
    // Cards that aren't on the battlefield and spells aren't permanents (its
    // 2004-12-01 ruling).
    assert!(!has_type(&state, spell, CardType::Artifact));
    assert!(!has_type(&state, card, CardType::Artifact));
    pool(&mut state, P0, &[ManaColor::G], 0);
    cast(&mut state, spell, &[]);
    assert_eq!(zone(&state, spell), Zone::Stack);
    assert!(!has_type(&state, spell, CardType::Artifact));
    settled(&mut state);
    assert!(has_type(&state, spell, CardType::Artifact));

    // The types last only while the Lattice is on the battlefield.
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(lattice, Zone::Graveyard),
    );
    for object in permanents {
        assert!(!has_type(&state, object, CardType::Artifact));
    }
}

#[test]
fn a_later_type_or_color_setting_effect_overwrites_mycosynth_lattice() {
    // Oko's +1 after the Lattice: P1's Island is an artifact, so Oko may
    // target it, and becomes a green Elk creature and nothing else (613.7).
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let island = put(&mut state, P1, "Island", Zone::Battlefield);
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    // Colorless mana pays {G}{U} as though it were any color.
    pool(&mut state, P0, &[], 3);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 1, &[Target::Object(island)]);
    settled(&mut state);
    assert!(has_type(&state, island, CardType::Creature));
    assert!(!has_type(&state, island, CardType::Artifact));
    assert!(!has_type(&state, island, CardType::Land));
    assert_eq!(color(&state, island), mana_color_mask(ManaColor::G));
    assert_eq!(color(&state, oko), 0);

    // The Lattice after an Elk: an artifact Elk, and colorless.
    let mut state = islands();
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let oko = put(&mut state, P0, "Oko, Thief of Crowns", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::G, ManaColor::U], 1);
    cast(&mut state, oko, &[]);
    settled(&mut state);
    activate(&mut state, oko, 1, &[Target::Object(elves)]);
    settled(&mut state);
    assert!(!has_type(&state, elves, CardType::Artifact));
    assert_ne!(color(&state, elves), 0);
    entered(&mut state, P0, LATTICE);
    assert!(has_type(&state, elves, CardType::Artifact));
    assert!(has_type(&state, elves, CardType::Creature));
    assert_eq!(color(&state, elves), 0);

    // Witness Protection after the Lattice: "is a green and white Citizen
    // creature" sets its host's types and colors again.
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let protection = put(&mut state, P0, "Witness Protection", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, protection, &[Target::Object(elves)]);
    settled(&mut state);
    assert_eq!(zone(&state, protection), Zone::Battlefield);
    assert!(has_type(&state, protection, CardType::Artifact));
    assert!(has_type(&state, elves, CardType::Creature));
    assert!(!has_type(&state, elves, CardType::Artifact));
    assert_eq!(
        color(&state, elves),
        mana_color_mask(ManaColor::G) | mana_color_mask(ManaColor::W)
    );

    // Wild Mongrel's color, chosen after the Lattice.
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let mongrel = put(&mut state, P0, "Wild Mongrel", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Graveyard);
    assert_eq!(color(&state, mongrel), 0);
    put(&mut state, P0, "Island", Zone::Hand);
    activate(&mut state, mongrel, 0, &[]);
    // White, blue, black, red, green: red.
    settle_picking(&mut state, 3);
    assert_eq!(color(&state, mongrel), mana_color_mask(ManaColor::R));
    assert_eq!(color(&state, bolt), 0);
}

#[test]
fn karn_and_null_rod_lock_every_permanent_under_mycosynth_lattice() {
    // Karn, the Great Creator: "Activated abilities of artifacts your
    // opponents control can't be activated." Under a Lattice that covers an
    // opponent's lands and planeswalkers too.
    let mut state = islands();
    entered(&mut state, P0, "Karn, the Great Creator");
    let own_island = put(&mut state, P0, "Island", Zone::Battlefield);
    let their_island = put(&mut state, P1, "Island", Zone::Battlefield);
    let oko = entered(&mut state, P1, "Oko, Thief of Crowns");
    opponents_main_phase(&mut state);
    assert!(activatable(&mut state, oko, 0), "not an artifact yet");
    entered(&mut state, P0, LATTICE);
    assert!(!activatable(&mut state, oko, 0));
    assert!(engine::step(&mut state, Action::ActivateManaAbility(their_island)).is_err());
    state.active_player = P0;
    state.priority_player = P0;
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(own_island));
    assert_eq!(state.players[0].mana_pool[ManaColor::U.pool_index()], 1);

    // Null Rod: "Activated abilities of artifacts can't be activated."
    let mut state = islands();
    put(&mut state, P1, "Null Rod", Zone::Battlefield);
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    assert!(castable(&mut state, bolt));
    entered(&mut state, P1, LATTICE);
    assert!(!castable(&mut state, bolt), "the Mountain is an artifact");
}

#[test]
fn mycosynth_lattice_counts_every_permanent_for_affinity() {
    let mut state = islands();
    for _ in 0..6 {
        let island = put(&mut state, P0, "Island", Zone::Battlefield);
        state.objects.get_mut(island).tapped = true;
    }
    put(&mut state, P1, "Island", Zone::Battlefield);
    let enforcer = put(&mut state, P0, "Myr Enforcer", Zone::Hand);
    assert!(!castable(&mut state, enforcer));
    // Six Islands and the Lattice: affinity for artifacts takes all {7}.
    entered(&mut state, P0, LATTICE);
    assert!(castable(&mut state, enforcer));
}

#[test]
fn a_land_made_an_artifact_pays_a_bargain() {
    // Bargain: "You may sacrifice an artifact, enchantment, or token as you
    // cast this spell." An Island is one under an opponent's Lattice, and
    // once Liquimetal Coating (wave S1b) makes it one.
    for lattice in [true, false] {
        let mut state = islands();
        let beseech = put(&mut state, P0, "Beseech the Mirror", Zone::Hand);
        let island = put(&mut state, P0, "Island", Zone::Battlefield);
        let ritual = put(&mut state, P0, "Dark Ritual", Zone::Library);
        if lattice {
            entered(&mut state, P1, LATTICE);
            pool(&mut state, P0, &[], 4);
        } else {
            let coating = put(&mut state, P0, "Liquimetal Coating", Zone::Battlefield);
            activate(&mut state, coating, 0, &[Target::Object(island)]);
            settled(&mut state);
            pool(
                &mut state,
                P0,
                &[ManaColor::B, ManaColor::B, ManaColor::B],
                1,
            );
        }
        assert!(has_type(&state, island, CardType::Artifact));
        act(&mut state, Action::CastSpell(beseech));
        settle_with(&mut state, |_, decision| match decision {
            Decision::ChooseEffectOption { source, .. } if source == beseech => {
                Action::ChooseEffectOption(1)
            }
            Decision::ChooseCostTargets { candidates, .. } => {
                assert!(candidates.contains(&island), "{candidates:?}");
                Action::ChooseCostTarget(island)
            }
            Decision::ChooseEffectTargets { legal_targets, .. }
                if legal_targets.contains(&Target::Object(ritual)) =>
            {
                Action::ChooseEffectTarget(Target::Object(ritual))
            }
            other => first_answer(other),
        });
        assert_eq!(zone(&state, island), Zone::Graveyard, "lattice {lattice}");
        assert_eq!(zone(&state, beseech), Zone::Graveyard);
        assert_eq!(
            zone(&state, ritual),
            Zone::Graveyard,
            "cast free and resolved"
        );
    }
}

// ---- Mycosynth Lattice: colors --------------------------------------------------

#[test]
fn mycosynth_lattice_makes_every_object_colorless() {
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let elves = put(&mut state, P1, "Llanowar Elves", Zone::Battlefield);
    let buried = put(&mut state, P1, "Counterspell", Zone::Graveyard);
    let top = put(&mut state, P0, "Force of Will", Zone::Library);
    for object in [bolt, elves, buried, top] {
        assert_ne!(color(&state, object), 0);
    }
    let lattice = entered(&mut state, P0, LATTICE);
    for object in [bolt, elves, buried, top, lattice] {
        assert_eq!(
            color(&state, object),
            0,
            "{}",
            state.objects.get(object).name
        );
    }
    // Blue mana pays {R}; the spell is colorless too.
    pool(&mut state, P0, &[ManaColor::U], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    assert_eq!(zone(&state, bolt), Zone::Stack);
    assert_eq!(color(&state, bolt), 0);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
}

#[test]
fn painters_servant_adds_its_color_only_after_mycosynth_lattice() {
    let mut state = islands();
    // An earlier Painter's Servant: the Lattice's later setting wins.
    let earlier = put(&mut state, P1, "Painter's Servant", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(earlier, Zone::Battlefield),
    );
    // White, blue, black, red, green: green.
    settle_picking(&mut state, 4);
    assert_eq!(
        state.objects.get(earlier).v4.chosen_color,
        Some(ManaColor::G)
    );
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    assert_ne!(color(&state, bolt) & mana_color_mask(ManaColor::G), 0);
    entered(&mut state, P0, LATTICE);
    assert_eq!(color(&state, bolt), 0);

    // A later one adds its color to everything (613.7).
    let painter = put(&mut state, P0, "Painter's Servant", Zone::Hand);
    pool(&mut state, P0, &[], 2);
    cast(&mut state, painter, &[]);
    // White, blue, black, red, green: blue.
    settle_picking(&mut state, 1);
    let blue = mana_color_mask(ManaColor::U);
    assert_eq!(color(&state, bolt), blue);
    assert_eq!(color(&state, painter), blue);
    assert_eq!(color(&state, earlier), blue);
}

/// Whether P0, holding no mana with Force of Will and Brainstorm in hand,
/// may cast Force of Will against P1's Lightning Bolt.
fn force_of_will_castable(lattice: bool) -> bool {
    let mut state = islands();
    if lattice {
        entered(&mut state, P1, LATTICE);
    }
    let force = put(&mut state, P0, "Force of Will", Zone::Hand);
    put(&mut state, P0, "Brainstorm", Zone::Hand);
    let bolt = put(&mut state, P1, "Lightning Bolt", Zone::Hand);
    opponents_main_phase(&mut state);
    pool(&mut state, P1, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P0)]);
    match next(&mut state) {
        Decision::CastSpellOrPass { player: P1, .. } => act(&mut state, Action::Pass),
        other => panic!("expected P1's priority, got {other:?}"),
    }
    let castable = castable(&mut state, force);
    assert_eq!(state.priority_player, P0);
    castable
}

#[test]
fn mycosynth_lattice_leaves_no_blue_card_for_force_of_will() {
    assert!(force_of_will_castable(false), "exiling Brainstorm");
    assert!(!force_of_will_castable(true), "Brainstorm is colorless");
}

// ---- Mycosynth Lattice: spending mana -------------------------------------------

#[test]
fn mycosynth_lattice_lets_any_mana_pay_a_colored_symbol() {
    // An Island taps for {R}: spells and abilities alike.
    let mut state = islands();
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let lavamancer = put(&mut state, P0, "Grim Lavamancer", Zone::Battlefield);
    for _ in 0..2 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    put(&mut state, P0, "Island", Zone::Battlefield);
    put(&mut state, P0, "Island", Zone::Battlefield);
    assert!(!castable(&mut state, bolt));
    assert!(!activatable(&mut state, lavamancer, 0));
    // Either player's Lattice.
    entered(&mut state, P1, LATTICE);
    assert!(castable(&mut state, bolt));
    assert!(activatable(&mut state, lavamancer, 0));
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 17);
    act(&mut state, Action::ActivateAbility(lavamancer, 0));
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseTargets { .. } => Action::ChooseTarget(Target::Player(P1)),
        other => first_answer(other),
    });
    assert_eq!(state.players[1].life, 15);

    // Floating colorless mana pays {U}.
    let mut state = islands();
    let recall = put(&mut state, P0, "Ancestral Recall", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    assert!(!castable(&mut state, recall));
    entered(&mut state, P0, LATTICE);
    assert!(castable(&mut state, recall));
}

#[test]
fn mycosynth_lattice_still_asks_colorless_mana_for_a_colorless_symbol() {
    // Thought-Knot Seer costs {3}{C}.
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let seer = put(&mut state, P0, "Thought-Knot Seer", Zone::Hand);
    for _ in 0..4 {
        put(&mut state, P0, "Island", Zone::Battlefield);
    }
    assert!(!castable(&mut state, seer), "blue mana isn't colorless");
    let tomb = put(&mut state, P0, "Ancient Tomb", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(tomb));
    assert_eq!(state.players[0].mana_pool[ManaColor::C.pool_index()], 2);
    assert!(castable(&mut state, seer));
}

#[test]
fn mycosynth_lattice_keeps_restrictions_and_delve_rules() {
    // Mishra's Workshop's mana still pays only for artifact spells, and a
    // spell isn't a permanent.
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    let ring = put(&mut state, P0, "Sol Ring", Zone::Hand);
    let workshop = put(&mut state, P0, "Mishra's Workshop", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(workshop));
    assert_eq!(state.players[0].restricted_mana_pool.0.len(), 3);
    let (castable_spells, _, _) = priority(&mut state);
    assert!(!castable_spells.contains(&bolt));
    assert!(castable_spells.contains(&ring));

    // Delve pays only generic mana: Treasure Cruise's {U} needs mana, any
    // color of it.
    let mut state = islands();
    entered(&mut state, P0, LATTICE);
    let cruise = put(&mut state, P0, "Treasure Cruise", Zone::Hand);
    for _ in 0..8 {
        put(&mut state, P0, "Island", Zone::Graveyard);
    }
    assert!(!castable(&mut state, cruise));
    put(&mut state, P0, "Mountain", Zone::Battlefield);
    assert!(castable(&mut state, cruise));
}

// ---- Overencumbered ---------------------------------------------------------------

/// P0 casts Overencumbered at P1, who controls Gurmag Angler and `lands`
/// untapped Islands. Returns the Aura and the Angler.
fn encumbered(lands: usize) -> (GameState, ObjectId, ObjectId) {
    let mut state = islands();
    let angler = put(&mut state, P1, "Gurmag Angler", Zone::Battlefield);
    for _ in 0..lands {
        put(&mut state, P1, "Island", Zone::Battlefield);
    }
    let aura = put(&mut state, P0, OVERENCUMBERED, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::W], 1);
    assert!(castable(&mut state, aura));
    act(&mut state, Action::CastSpell(aura));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert_eq!(legal_targets, vec![Target::Player(P1)], "enchant opponent");
        }
        other => panic!("expected targets, got {other:?}"),
    }
    act(&mut state, Action::ChooseTarget(Target::Player(P1)));
    settled(&mut state);
    (state, aura, angler)
}

/// Plays on to P1's declaration of attackers, answering the combat
/// trigger's choice (pay, or creatures can't attack) with `answer`.
/// Returns the creatures P1 may attack with.
fn p1_attackers(state: &mut GameState, answer: Option<u16>) -> Vec<ObjectId> {
    let mut asked = false;
    run_until(
        state,
        |state| state.active_player == P1 && state.step == Step::DeclareAttackers,
        |_, decision| match decision {
            Decision::ChooseEffectOption {
                player,
                option_count,
                ..
            } => {
                assert_eq!((*player, *option_count), (P1, 2), "pay or don't");
                asked = true;
                Some(Action::ChooseEffectOption(
                    answer.expect("no payment is offered"),
                ))
            }
            _ => None,
        },
    );
    assert_eq!(asked, answer.is_some());
    match next(state) {
        Decision::DeclareAttackers { player, eligible } => {
            assert_eq!(player, P1);
            eligible
        }
        other => panic!("expected attackers, got {other:?}"),
    }
}

#[test]
fn overencumbered_enchants_an_opponent_who_creates_a_clue_a_food_and_a_junk() {
    let (state, aura, _) = encumbered(0);
    assert_eq!(zone(&state, aura), Zone::Battlefield);
    assert_eq!(state.objects.get(aura).controller, P0);
    assert_eq!(enchanted(&state), vec![P1]);
    for name in TOKENS {
        let tokens = named(&state, P1, name);
        assert_eq!(tokens.len(), 1, "{name}");
        assert!(state.objects.get(tokens[0]).v4.is_token);
        assert!(has_type(&state, tokens[0], CardType::Artifact));
        assert!(named(&state, P0, name).is_empty(), "{name}");
    }
    let junk = named(&state, P1, JUNK)[0];
    assert!(engine::has_effective_subtype(&state, junk, Subtype::Junk));
    let copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    assert_eq!(enchanted(&copy), vec![P1]);
}

#[test]
fn overencumbered_asks_its_player_to_pay_for_each_artifact_or_not_attack() {
    // Paid: {3} for the Clue, the Food and the Junk.
    let (mut state, _, angler) = encumbered(3);
    assert_eq!(p1_attackers(&mut state, Some(0)), vec![angler]);
    let islands_tapped = named(&state, P1, "Island")
        .into_iter()
        .filter(|&island| state.objects.get(island).tapped)
        .count();
    assert_eq!(islands_tapped, 3);

    // Declined: creatures can't attack this combat.
    let (mut state, aura, angler) = encumbered(3);
    let attacker = put(&mut state, P0, "Llanowar Elves", Zone::Battlefield);
    assert!(p1_attackers(&mut state, Some(1)).is_empty());
    assert!(named(&state, P1, "Island")
        .into_iter()
        .all(|island| !state.objects.get(island).tapped));
    let mut copy = restored(&state);
    assert_eq!(state.state_hash(), copy.state_hash());
    for continued in [&mut state, &mut copy] {
        assert!(matches!(
            next(continued),
            Decision::DeclareAttackers { eligible, .. } if eligible.is_empty()
        ));
        act(continued, Action::DeclareAttackers(Vec::new()));
        // The ban ends with the turn, and the trigger is only for the
        // enchanted player's turn.
        pass_until(continued, |state| {
            state.active_player == P0 && state.step == Step::DeclareAttackers
        });
        assert!(matches!(
            next(continued),
            Decision::DeclareAttackers { eligible, .. } if eligible == vec![attacker]
        ));
        assert!(read_state(continued).is_some_and(|s1f| s1f.attack_bans.is_empty()));
        assert_eq!(zone(continued, aura), Zone::Battlefield);
        assert_eq!(zone(continued, angler), Zone::Battlefield);
    }

    // Two Islands can't pay {3}: no choice, and no attack.
    let (mut state, _, _) = encumbered(2);
    assert!(p1_attackers(&mut state, None).is_empty());

    // No artifacts: {0} is paid.
    let (mut state, _, angler) = encumbered(0);
    for name in TOKENS {
        let token = named(&state, P1, name)[0];
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(token, Zone::Graveyard),
        );
    }
    assert_eq!(p1_attackers(&mut state, None), vec![angler]);
}

#[test]
fn overencumbered_goes_to_the_graveyard_once_it_enchants_its_controller() {
    let (mut state, aura, _) = encumbered(0);
    give_control(&mut state, aura, P1);
    mtg_kernel::trigger::sba_fixed_point(&mut state);
    assert_eq!(zone(&state, aura), Zone::Graveyard, "704.5m");
    // The record is dropped as the next turn begins.
    pass_until(&mut state, |state| state.active_player == P1);
    assert!(enchanted(&state).is_empty());
}

#[test]
fn overencumbered_put_onto_the_battlefield_enchants_its_controllers_opponent() {
    // Show and Tell offers it (303.4f): P1 puts it onto the battlefield.
    let mut state = islands();
    let show = put(&mut state, P0, "Show and Tell", Zone::Hand);
    let aura = put(&mut state, P1, OVERENCUMBERED, Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 2);
    cast(&mut state, show, &[]);
    settle_with(&mut state, |_, decision| match decision {
        Decision::ChooseEffectTargets {
            player: P1,
            legal_targets,
            selected_count: 0,
            ..
        } => {
            assert_eq!(legal_targets, vec![Target::Object(aura)]);
            Action::ChooseEffectTarget(Target::Object(aura))
        }
        other => first_answer(other),
    });
    assert_eq!(zone(&state, aura), Zone::Battlefield);
    assert_eq!(state.objects.get(aura).controller, P1);
    assert_eq!(enchanted(&state), vec![P0]);
    for name in TOKENS {
        assert_eq!(named(&state, P0, name).len(), 1, "{name}");
    }

    // Replenish returns it with the other enchantments.
    let mut state = islands();
    let replenish = put(&mut state, P0, "Replenish", Zone::Hand);
    let aura = put(&mut state, P0, OVERENCUMBERED, Zone::Graveyard);
    pool(&mut state, P0, &[ManaColor::W], 3);
    cast(&mut state, replenish, &[]);
    settle_with(&mut state, |_, decision| first_answer(decision));
    assert_eq!(zone(&state, aura), Zone::Battlefield);
    assert_eq!(enchanted(&state), vec![P1]);
    for name in TOKENS {
        assert_eq!(named(&state, P1, name).len(), 1, "{name}");
    }
}

// ---- Junk token -------------------------------------------------------------------

#[test]
fn the_junk_token_exiles_the_top_card_to_play_this_turn_as_a_sorcery() {
    let mut state = islands();
    let junk = token(&mut state, P0, JUNK);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    assert_eq!(state.players[0].library[0], bolt);
    // Not while a spell is on the stack.
    let other = put(&mut state, P0, "Lightning Bolt", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, other, &[Target::Player(P1)]);
    assert!(!activatable(&mut state, junk, 0));
    settled(&mut state);
    activate(&mut state, junk, 0, &[]);
    settled(&mut state);
    assert!(named(&state, P0, JUNK).is_empty(), "sacrificed");
    assert_eq!(zone(&state, bolt), Zone::Exile);
    assert!(!castable(&mut state, bolt), "its cost still applies");
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, bolt, &[Target::Player(P1)]);
    settled(&mut state);
    assert_eq!(state.players[1].life, 14);

    // A land is played with the turn's land drop.
    let mut state = islands();
    let junk = token(&mut state, P0, JUNK);
    let top = state.players[0].library[0];
    activate(&mut state, junk, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, top), Zone::Exile);
    assert!(priority(&mut state).2.contains(&top));
    act(&mut state, Action::PlayLand(top));
    assert_eq!(zone(&state, top), Zone::Battlefield);

    // Not on an opponent's turn, and the permission ends with the turn.
    let mut state = islands();
    let junk = token(&mut state, P0, JUNK);
    let bolt = put(&mut state, P0, "Lightning Bolt", Zone::Library);
    activate(&mut state, junk, 0, &[]);
    settled(&mut state);
    assert_eq!(zone(&state, bolt), Zone::Exile);
    let second = token(&mut state, P0, JUNK);
    pass_until(&mut state, |state| {
        state.active_player == P1 && state.step == Step::Main1 && state.priority_player == P0
    });
    pool(&mut state, P0, &[ManaColor::R], 0);
    let (castable_spells, abilities, _) = priority(&mut state);
    assert!(!castable_spells.contains(&bolt));
    assert!(!abilities.contains(&(second, 0)));
    assert_eq!(zone(&state, bolt), Zone::Exile);
}
