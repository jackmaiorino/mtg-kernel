//! Eternal wave E1: native rules tests for the fourteen Pauper cards that
//! complete Mono-Red Madness and Rally, Grixis Affinity, Dimir and Grixis
//! Faeries, and Grixis and Dimir Control, plus the Robot token.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, CostKind, Decision, OptionalCostChoice};
use mtg_kernel::eternal_cards_v1::{energy, replicate_cost};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const E1_CARDS: [&str; 15] = [
    "Melded Moxite",
    "Robot Token",
    "Inventor's Axe",
    "Baleful Strix",
    "Giant's Boulder",
    "Utrom Monitor",
    "Agony Warp",
    "Augur of Bolas",
    "Mukotai Ambusher",
    "Seething Landscape",
    "Thorn of the Black Rose",
    "Campfire",
    "Lose Focus",
    "Plunder the Trollshaws",
    "Dimir Aqueduct",
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
        0x4531_5041_5550,
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

/// Settles, answering every library or scry selection by finishing with
/// whatever is already selected and every order by the offered order.
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
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(
                    legal_targets.contains(target),
                    "{target:?} not in {legal_targets:?}"
                );
                act(state, Action::ChooseTarget(*target));
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

fn named_on_battlefield(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| CARD_DEFS[state.objects.get(id).card_def as usize].name == name)
        .collect()
}

#[test]
fn e1_cards_are_full_with_their_printed_characteristics() {
    for name in E1_CARDS {
        assert_eq!(def(name).capability, CardCapability::Full, "{name}");
    }

    let strix = def("Baleful Strix");
    assert_eq!(strix.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(strix.subtypes, &[Subtype::Bird]);
    assert_eq!(strix.colors, &[ManaColor::U, ManaColor::B]);
    assert_eq!((strix.power, strix.toughness), (Some(1), Some(1)));
    assert!(strix.keywords.has(Keywords::FLYING));
    assert!(strix.keywords.has(Keywords::DEATHTOUCH));

    let monitor = def("Utrom Monitor");
    assert_eq!(monitor.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(monitor.subtypes, &[Subtype::Utrom, Subtype::Scientist]);
    assert_eq!((monitor.power, monitor.toughness), (Some(3), Some(3)));
    assert_eq!(monitor.mana_value, 5);
    assert!(monitor.keywords.has(Keywords::FLYING));
    assert!(monitor.generic_cost_reduction.is_some());
    assert_eq!(
        monitor.generic_cost_reduction,
        def("Myr Enforcer").generic_cost_reduction
    );

    let ambusher = def("Mukotai Ambusher");
    assert_eq!(ambusher.subtypes, &[Subtype::Rat, Subtype::Ninja]);
    assert!(ambusher.keywords.has(Keywords::LIFELINK));

    let thorn = def("Thorn of the Black Rose");
    assert_eq!(thorn.subtypes, &[Subtype::Human, Subtype::Assassin]);
    assert!(thorn.keywords.has(Keywords::DEATHTOUCH));
    assert_eq!((thorn.power, thorn.toughness), (Some(1), Some(3)));

    let augur = def("Augur of Bolas");
    assert_eq!(augur.subtypes, &[Subtype::Merfolk, Subtype::Wizard]);
    assert_eq!((augur.power, augur.toughness), (Some(1), Some(3)));

    let axe = def("Inventor's Axe");
    assert_eq!(axe.subtypes, &[Subtype::Equipment]);
    assert!(axe.keywords.has(Keywords::FLASH));
    let equipment = axe.equipment.expect("Inventor's Axe is Equipment");
    assert_eq!((equipment.power_delta, equipment.toughness_delta), (2, 0));

    let robot = def("Robot Token");
    assert!(robot.is_token);
    assert_eq!(robot.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(robot.subtypes, &[Subtype::Robot]);
    assert!(robot.colors.is_empty());
    assert_eq!((robot.power, robot.toughness), (Some(2), Some(2)));
    for subtype in [Subtype::Utrom, Subtype::Scientist, Subtype::Robot] {
        assert!(subtype.is_creature_type(), "{subtype:?}");
    }

    assert!(def("Dimir Aqueduct").enters_battlefield_tapped);
    assert!(def("Dimir Aqueduct").is_land);
    assert!(def("Seething Landscape").is_land);
    assert!(def("Plunder the Trollshaws").flashback.is_some());
    assert_eq!(def("Lose Focus").mana_value, 2);
    assert!(replicate_cost("Lose Focus").is_some());
    assert!(replicate_cost("Make Disappear").is_none());
}

// ---- Baleful Strix, Thorn of the Black Rose, Utrom Monitor ----------------

#[test]
fn baleful_strix_draws_a_card_when_it_enters() {
    let mut state = islands();
    let strix = put(&mut state, P0, "Baleful Strix", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 0);
    let library = state.players[0].library.len();
    cast(&mut state, strix, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(strix).zone, Zone::Battlefield);
    assert_eq!(state.players[0].library.len(), library - 1);
    assert_eq!(state.players[0].hand.len(), 1);
    assert!(engine::has_effective_keyword(
        &state,
        strix,
        Keywords::FLYING
    ));
    assert!(engine::has_effective_keyword(
        &state,
        strix,
        Keywords::DEATHTOUCH
    ));
}

#[test]
fn thorn_of_the_black_rose_makes_its_controller_the_monarch() {
    let mut state = islands();
    state.monarch = Some(P1);
    let thorn = put(&mut state, P0, "Thorn of the Black Rose", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 3);
    cast(&mut state, thorn, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(thorn).zone, Zone::Battlefield);
    assert_eq!(state.monarch, Some(P0));
    assert!(engine::has_effective_keyword(
        &state,
        thorn,
        Keywords::DEATHTOUCH
    ));
}

#[test]
fn utrom_monitor_has_affinity_for_artifacts_and_flies() {
    let mut state = islands();
    for _ in 0..3 {
        put(&mut state, P0, "Ichor Wellspring", Zone::Battlefield);
    }
    let monitor = put(&mut state, P0, "Utrom Monitor", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 0);
    assert!(
        !castable(&mut state, monitor),
        "{{4}}{{U}} less three is {{1}}{{U}}"
    );
    pool(&mut state, P0, &[], 1);
    cast(&mut state, monitor, &[]);
    settled(&mut state);
    assert_eq!(state.objects.get(monitor).zone, Zone::Battlefield);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert!(engine::has_effective_keyword(
        &state,
        monitor,
        Keywords::FLYING
    ));
    assert_eq!(power_toughness(&state, monitor), (3, 3));
}

// ---- Mukotai Ambusher -------------------------------------------------------

#[test]
fn mukotai_ambusher_ninjutsus_in_attacking_and_has_lifelink() {
    let mut state = game(&[]);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let attacker = put(&mut state, P0, "Faerie Seer", Zone::Battlefield);
    state.objects.get_mut(attacker).tapped = true;
    state.engine.combat.attackers = vec![attacker];
    let ambusher = put(&mut state, P0, "Mukotai Ambusher", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::B], 0);
    assert!(
        !activatable(&mut state, ambusher, 0),
        "ninjutsu costs {{1}}{{B}}"
    );
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, ambusher, 0));
    act(&mut state, Action::ActivateAbility(ambusher, 0));
    match next(&mut state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
            assert_eq!(candidates, vec![attacker]);
            act(&mut state, Action::ChooseCostTarget(attacker));
        }
        Decision::CastSpellOrPass { .. } => {}
        other => panic!("ninjutsu return cost: {other:?}"),
    }
    assert_eq!(state.objects.get(attacker).zone, Zone::Hand);
    settled(&mut state);
    assert_eq!(state.objects.get(ambusher).zone, Zone::Battlefield);
    assert!(state.objects.get(ambusher).tapped);
    assert!(state.engine.combat.attackers.contains(&ambusher));

    let (life0, life1) = (state.players[0].life, state.players[1].life);
    for _ in 0..32 {
        if state.step == Step::CombatDamage || state.step == Step::EndCombat {
            break;
        }
        match next(&mut state) {
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    next(&mut state);
    assert_eq!(state.players[1].life, life1 - 3);
    assert_eq!(state.players[0].life, life0 + 3, "lifelink");
}

// ---- Agony Warp ---------------------------------------------------------------

#[test]
fn agony_warp_targets_two_creatures_which_may_be_the_same() {
    // Both halves on one creature: a 4/4 becomes 1/1.
    let mut state = islands();
    let enforcer = put(&mut state, P1, "Myr Enforcer", Zone::Battlefield);
    let warp = put(&mut state, P0, "Agony Warp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 0);
    cast(
        &mut state,
        warp,
        &[Target::Object(enforcer), Target::Object(enforcer)],
    );
    assert_eq!(
        state.stack.last().unwrap().targets,
        vec![Target::Object(enforcer), Target::Object(enforcer)]
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, enforcer), (1, 1));
    assert_eq!(state.objects.get(warp).zone, Zone::Graveyard);

    // Two creatures: -3/-0 on the first, -0/-3 on the second, which dies.
    let mut state = islands();
    let enforcer = put(&mut state, P1, "Myr Enforcer", Zone::Battlefield);
    let thorn = put(&mut state, P1, "Thorn of the Black Rose", Zone::Battlefield);
    let warp = put(&mut state, P0, "Agony Warp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 0);
    cast(
        &mut state,
        warp,
        &[Target::Object(enforcer), Target::Object(thorn)],
    );
    settled(&mut state);
    assert_eq!(power_toughness(&state, enforcer), (1, 4));
    assert_eq!(state.objects.get(thorn).zone, Zone::Graveyard);
}

#[test]
fn agony_warp_still_affects_its_legal_target_when_the_other_is_gone() {
    let mut state = islands();
    let enforcer = put(&mut state, P1, "Myr Enforcer", Zone::Battlefield);
    let thorn = put(&mut state, P1, "Thorn of the Black Rose", Zone::Battlefield);
    let warp = put(&mut state, P0, "Agony Warp", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 0);
    cast(
        &mut state,
        warp,
        &[Target::Object(thorn), Target::Object(enforcer)],
    );
    // The -3/-0 target leaves before resolution (608.2b).
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(thorn, Zone::Hand));
    settled(&mut state);
    assert_eq!(power_toughness(&state, enforcer), (4, 1));
    assert_eq!(state.objects.get(warp).zone, Zone::Graveyard);
}

// ---- Augur of Bolas -------------------------------------------------------------

fn augur_with_top(top: &[&str]) -> (GameState, ObjectId, Vec<ObjectId>) {
    let mut library = top.to_vec();
    library.extend(["Island"; 10]);
    let mut state = game(&library);
    let augur = put(&mut state, P0, "Augur of Bolas", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    let looked = state.players[0].library[..3].to_vec();
    cast(&mut state, augur, &[]);
    (state, augur, looked)
}

#[test]
fn augur_of_bolas_may_reveal_an_instant_or_sorcery_and_bottoms_the_rest() {
    let (mut state, augur, looked) = augur_with_top(&["Island", "Lightning Bolt", "Ponder"]);
    let Some(Decision::ChooseEffectTargets {
        min_targets,
        max_targets,
        legal_targets,
        ..
    }) = settle(&mut state)
    else {
        panic!("missing the Augur selection")
    };
    assert_eq!((min_targets, max_targets), (0, 1));
    assert_eq!(
        legal_targets,
        vec![Target::Object(looked[1]), Target::Object(looked[2])],
        "only the instant and the sorcery are selectable"
    );
    // The look is private until the chosen card is revealed.
    assert!(state.known_hand_cards(P1, P0).is_empty());
    act(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(looked[1])),
    );
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(augur).zone, Zone::Battlefield);
    assert_eq!(state.players[0].hand, vec![looked[1]]);
    assert!(state
        .known_hand_cards(P1, P0)
        .iter()
        .any(|entry| entry.object == looked[1]));
    let library = &state.players[0].library;
    assert_eq!(library.len(), 12);
    let bottom = &library[library.len() - 2..];
    assert!(bottom.contains(&looked[0]) && bottom.contains(&looked[2]));
}

#[test]
fn augur_of_bolas_may_take_nothing() {
    let (mut state, _, looked) = augur_with_top(&["Lightning Bolt", "Island", "Island"]);
    settle_declining_selections(&mut state);
    assert!(state.players[0].hand.is_empty());
    assert!(state.known_hand_cards(P1, P0).is_empty());
    let library = &state.players[0].library;
    assert_eq!(library.len(), 13);
    for card in looked {
        assert!(library[library.len() - 3..].contains(&card));
    }
}

// ---- Lose Focus ---------------------------------------------------------------

/// P1 casts Burst Lightning at P0, then P0 answers with Lose Focus paying
/// replicate `copies` times. Returns Lose Focus and the Burst Lightning.
fn lose_focus_a_burn(state: &mut GameState, copies: u16) -> (ObjectId, ObjectId) {
    let focus = put(state, P0, "Lose Focus", Zone::Hand);
    state.priority_player = P1;
    let burst = put(state, P1, "Burst Lightning", Zone::Hand);
    pool(state, P1, &[ManaColor::R], 0);
    cast(state, burst, &[Target::Player(P0)]);
    assert!(matches!(
        next(state),
        Decision::CastSpellOrPass { player: P1, .. }
    ));
    act(state, Action::Pass);
    let blue = vec![ManaColor::U; 1 + usize::from(copies)];
    pool(state, P0, &blue, 1);
    assert!(castable(state, focus));
    act(state, Action::CastSpell(focus));
    let mut announced = false;
    for _ in 0..5 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(burst)));
                act(state, Action::ChooseTarget(Target::Object(burst)));
            }
            Decision::ChooseEffectOption { option_count, .. } => {
                // Option i announces i replicate payments.
                assert_eq!(option_count, copies + 1);
                act(state, Action::ChooseEffectOption(copies));
                announced = true;
            }
            Decision::CastSpellOrPass { .. } => break,
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert_eq!(announced, copies > 0);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    (focus, burst)
}

/// Resolves the stack: P0 keeps every copy's target, and P1 pays every
/// "unless its controller pays {2}" it can afford.
fn resolve_counters(state: &mut GameState) -> usize {
    let mut retargets = 0;
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return retargets
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::ChooseEffectTargets { player, .. } => {
                assert_eq!(player, P0);
                retargets += 1;
                act(state, Action::FinishEffectSelection);
            }
            Decision::ChooseEffectBoolean { player, .. } => {
                assert_eq!(player, P1);
                act(state, Action::ChooseEffectBoolean(true));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("stack did not resolve");
}

#[test]
fn lose_focus_without_replicate_is_one_counter_unless_pays_two() {
    let mut state = islands();
    let (_, burst) = lose_focus_a_burn(&mut state, 0);
    assert_eq!(state.stack.len(), 2);
    assert!(state.engine.pending_triggers.is_empty());
    pool(&mut state, P1, &[], 2);
    assert_eq!(resolve_counters(&mut state), 0);
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 18, "P1 paid {{2}}");
}

#[test]
fn lose_focus_replicate_copies_once_per_payment() {
    let mut state = islands();
    let (focus, burst) = lose_focus_a_burn(&mut state, 2);
    let original = state
        .stack
        .iter()
        .find(|item| item.source == focus)
        .unwrap()
        .clone();
    assert!(!original.is_copy);
    assert_eq!(original.v4.x_value, 2);
    assert_eq!(
        engine::object_mana_value(&state, focus),
        2,
        "replicate payments do not change the mana value"
    );
    // P1 can pay for the two copies but not for the original.
    pool(&mut state, P1, &[], 4);
    assert_eq!(resolve_counters(&mut state), 2, "one retarget per copy");
    assert_eq!(state.objects.get(burst).zone, Zone::Graveyard);
    assert_eq!(state.players[0].life, 20, "the original countered the burn");
    assert_eq!(state.players[1].mana_pool, [0; 6]);
}

#[test]
fn lose_focus_replicate_copy_can_take_a_new_target() {
    let mut state = islands();
    let (_, burst) = lose_focus_a_burn(&mut state, 1);
    // A second P0-targeted spell joins the stack before the copy trigger.
    let copy_trigger = state.engine.pending_triggers.len() + state.stack.len();
    assert!(copy_trigger >= 3);
    let mut copy_target = None;
    for _ in 0..60 {
        match next(&mut state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                break
            }
            Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => act(
                &mut state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::ChooseEffectTargets {
                legal_targets,
                player,
                ..
            } => {
                assert_eq!(player, P0);
                // The copy may target the original Lose Focus instead.
                let focus = legal_targets
                    .iter()
                    .copied()
                    .find(|target| *target != Target::Object(burst))
                    .expect("another spell to retarget to");
                copy_target = Some(focus);
                act(&mut state, Action::ChooseEffectTarget(focus));
                if let Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } = next(&mut state)
                {
                    act(&mut state, Action::FinishEffectSelection);
                }
            }
            Decision::ChooseEffectBoolean { .. } => {
                act(&mut state, Action::ChooseEffectBoolean(false))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    assert!(copy_target.is_some());
    // The copy countered the original Lose Focus, so the burn resolved.
    assert_eq!(state.players[0].life, 18);
}

// ---- Plunder the Trollshaws -----------------------------------------------------

#[test]
fn plunder_the_trollshaws_draws_one_from_hand_and_two_with_flashback() {
    let mut state = islands();
    let plunder = put(&mut state, P0, "Plunder the Trollshaws", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U], 1);
    cast(&mut state, plunder, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 1);
    assert_eq!(state.objects.get(plunder).zone, Zone::Graveyard);

    // Flashback {3}{U} from the graveyard draws two, then exiles it.
    pool(&mut state, P0, &[ManaColor::U], 2);
    assert!(!castable(&mut state, plunder));
    pool(&mut state, P0, &[], 1);
    cast(&mut state, plunder, &[]);
    settled(&mut state);
    assert_eq!(state.players[0].hand.len(), 3);
    assert_eq!(state.objects.get(plunder).zone, Zone::Exile);
}

// ---- Campfire -------------------------------------------------------------------

#[test]
fn campfire_gains_two_life_for_one() {
    let mut state = islands();
    let campfire = put(&mut state, P0, "Campfire", Zone::Battlefield);
    state.players[0].life = 10;
    assert!(!activatable(&mut state, campfire, 0));
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, campfire, 0));
    act(&mut state, Action::ActivateAbility(campfire, 0));
    settled(&mut state);
    assert_eq!(state.players[0].life, 12);
    assert!(state.objects.get(campfire).tapped);
}

#[test]
fn campfire_exiles_itself_to_shuffle_the_graveyard_into_the_library() {
    let mut state = islands();
    let campfire = put(&mut state, P0, "Campfire", Zone::Battlefield);
    let graveyard = [
        put(&mut state, P0, "Lightning Bolt", Zone::Graveyard),
        put(&mut state, P0, "Counterspell", Zone::Graveyard),
    ];
    let theirs = put(&mut state, P1, "Lightning Bolt", Zone::Graveyard);
    let library = state.players[0].library.len();
    pool(&mut state, P0, &[], 2);
    assert!(activatable(&mut state, campfire, 1));
    act(&mut state, Action::ActivateAbility(campfire, 1));
    next(&mut state);
    assert_eq!(state.objects.get(campfire).zone, Zone::Exile);
    settled(&mut state);
    assert!(state.players[0].graveyard.is_empty());
    assert_eq!(state.players[0].library.len(), library + 2);
    for card in graveyard {
        assert_eq!(state.objects.get(card).zone, Zone::Library);
    }
    assert_eq!(state.objects.get(theirs).zone, Zone::Graveyard);
}

// ---- Giant's Boulder ------------------------------------------------------------

#[test]
fn giants_boulder_scries_on_entry_and_taps_for_any_color_for_one() {
    let mut state = islands();
    let boulder = put(&mut state, P0, "Giant's Boulder", Zone::Hand);
    pool(&mut state, P0, &[], 1);
    cast(&mut state, boulder, &[]);
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(boulder).zone, Zone::Battlefield);
    assert_eq!(state.players[0].library.len(), 20);

    // "{1}, {T}: Add one mana of any color." It is never free.
    let (_, _) = priority(&mut state);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(boulder, ManaColor::G)
    )
    .is_err());
    pool(&mut state, P0, &[], 1);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(boulder, ManaColor::G),
    );
    let mut green = [0; 6];
    green[ManaColor::G.pool_index()] = 1;
    assert_eq!(state.players[0].mana_pool, green);
    assert!(state.objects.get(boulder).tapped);
}

#[test]
fn giants_boulder_sacrifices_for_seven_to_destroy_target_permanent() {
    let mut state = islands();
    let boulder = put(&mut state, P0, "Giant's Boulder", Zone::Battlefield);
    let land = put(&mut state, P1, "Island", Zone::Battlefield);
    pool(&mut state, P0, &[], 6);
    assert!(!activatable(&mut state, boulder, 0));
    pool(&mut state, P0, &[], 1);
    assert!(activatable(&mut state, boulder, 0));
    act(&mut state, Action::ActivateAbility(boulder, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(land)));
            act(&mut state, Action::ChooseTarget(Target::Object(land)));
        }
        other => panic!("expected a target, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(boulder).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(land).zone, Zone::Graveyard);
}

// ---- Dimir Aqueduct ---------------------------------------------------------------

#[test]
fn dimir_aqueduct_enters_tapped_and_returns_a_land_you_control() {
    let mut state = islands();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let aqueduct = put(&mut state, P0, "Dimir Aqueduct", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(aqueduct));
    assert!(state.objects.get(aqueduct).tapped);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets.len(), 2);
            assert!(legal_targets.contains(&Target::Object(island)));
            assert!(legal_targets.contains(&Target::Object(aqueduct)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(island)),
            );
        }
        other => panic!("expected the land choice, got {other:?}"),
    }
    settled(&mut state);
    assert_eq!(state.objects.get(island).zone, Zone::Hand);
    assert_eq!(state.objects.get(aqueduct).zone, Zone::Battlefield);

    // Alone, it returns itself.
    let mut state = islands();
    let aqueduct = put(&mut state, P0, "Dimir Aqueduct", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(aqueduct));
    settled(&mut state);
    assert_eq!(state.objects.get(aqueduct).zone, Zone::Hand);
}

#[test]
fn dimir_aqueduct_taps_for_blue_and_black_at_once() {
    let mut state = islands();
    let aqueduct = put(&mut state, P0, "Dimir Aqueduct", Zone::Battlefield);
    next(&mut state);
    act(
        &mut state,
        Action::ActivateManaAbilityChoice(aqueduct, ManaColor::U),
    );
    let mut expected = [0; 6];
    expected[ManaColor::U.pool_index()] = 1;
    expected[ManaColor::B.pool_index()] = 1;
    assert_eq!(state.players[0].mana_pool, expected);
    assert!(state.objects.get(aqueduct).tapped);
    assert!(engine::step(
        &mut state,
        Action::ActivateManaAbilityChoice(aqueduct, ManaColor::U)
    )
    .is_err());
}

// ---- Seething Landscape -----------------------------------------------------------

#[test]
fn seething_landscape_taps_for_colorless_and_fetches_a_basic_tapped() {
    let mut state = game(&["Forest", "Swamp", "Forest", "Forest"]);
    let landscape = put(&mut state, P0, "Seething Landscape", Zone::Battlefield);
    next(&mut state);
    act(&mut state, Action::ActivateManaAbility(landscape));
    let mut colorless = [0; 6];
    colorless[ManaColor::C.pool_index()] = 1;
    assert_eq!(state.players[0].mana_pool, colorless);

    let mut state = game(&["Forest", "Swamp", "Forest", "Forest"]);
    let landscape = put(&mut state, P0, "Seething Landscape", Zone::Battlefield);
    let swamp = state.players[0].library[1];
    assert!(activatable(&mut state, landscape, 0));
    act(&mut state, Action::ActivateAbility(landscape, 0));
    next(&mut state);
    assert_eq!(state.objects.get(landscape).zone, Zone::Graveyard);
    match settle(&mut state) {
        Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
            assert_eq!(legal_targets, vec![Target::Object(swamp)]);
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(swamp)),
            );
        }
        other => panic!("expected the search, got {other:?}"),
    }
    settle_declining_selections(&mut state);
    assert_eq!(state.objects.get(swamp).zone, Zone::Battlefield);
    assert!(state.objects.get(swamp).tapped);
}

#[test]
fn seething_landscape_cycles_for_ubr() {
    let mut state = islands();
    let landscape = put(&mut state, P0, "Seething Landscape", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::U, ManaColor::B], 0);
    assert!(!activatable(&mut state, landscape, 1));
    pool(&mut state, P0, &[ManaColor::R], 0);
    assert!(activatable(&mut state, landscape, 1));
    act(&mut state, Action::ActivateAbility(landscape, 1));
    settled(&mut state);
    assert_eq!(state.objects.get(landscape).zone, Zone::Graveyard);
    assert_eq!(state.players[0].hand.len(), 1);
}

// ---- Melded Moxite and the Robot token ---------------------------------------------

#[test]
fn melded_moxite_may_discard_to_draw_two() {
    for accept in [false, true] {
        let mut state = islands();
        let moxite = put(&mut state, P0, "Melded Moxite", Zone::Hand);
        let fodder = put(&mut state, P0, "Island", Zone::Hand);
        pool(&mut state, P0, &[ManaColor::R], 1);
        cast(&mut state, moxite, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseOptionalCost {
                player,
                discard_payable,
                ..
            }) => {
                assert_eq!(player, P0);
                assert!(discard_payable);
            }
            other => panic!("expected the optional discard, got {other:?}"),
        }
        let choice = if accept {
            OptionalCostChoice::Discard
        } else {
            OptionalCostChoice::Decline
        };
        act(&mut state, Action::ChooseOptionalCost(choice));
        while let Some(decision) = settle(&mut state) {
            match decision {
                Decision::Discard { choices, .. } => {
                    assert!(choices.contains(&fodder));
                    act(&mut state, Action::Discard(vec![fodder]));
                }
                other => panic!("unexpected choice: {other:?}"),
            }
        }
        assert_eq!(state.objects.get(moxite).zone, Zone::Battlefield);
        if accept {
            assert_eq!(state.objects.get(fodder).zone, Zone::Graveyard);
            assert_eq!(state.players[0].hand.len(), 2);
        } else {
            assert_eq!(state.players[0].hand, vec![fodder]);
        }
    }
}

#[test]
fn melded_moxite_sacrifices_for_a_tapped_robot() {
    let mut state = islands();
    let moxite = put(&mut state, P0, "Melded Moxite", Zone::Battlefield);
    pool(&mut state, P0, &[], 2);
    assert!(!activatable(&mut state, moxite, 0));
    pool(&mut state, P0, &[], 1);
    act(&mut state, Action::ActivateAbility(moxite, 0));
    next(&mut state);
    assert_eq!(state.objects.get(moxite).zone, Zone::Graveyard);
    settled(&mut state);
    let robots = named_on_battlefield(&state, P0, "Robot Token");
    assert_eq!(robots.len(), 1);
    let robot = robots[0];
    assert!(state.objects.get(robot).tapped);
    assert!(state.objects.get(robot).v4.is_token);
    assert_eq!(power_toughness(&state, robot), (2, 2));
    assert!(engine::object_has_type(&state, robot, CardType::Artifact));
    assert!(engine::object_has_type(&state, robot, CardType::Creature));
}

// ---- Inventor's Axe and energy -----------------------------------------------------

#[test]
fn inventors_axe_gives_energy_attaches_on_entry_and_equips_for_energy() {
    let mut state = islands();
    let first = put(&mut state, P0, "Faerie Seer", Zone::Battlefield);
    let second = put(&mut state, P0, "Thorn of the Black Rose", Zone::Battlefield);
    let axe = put(&mut state, P0, "Inventor's Axe", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 0);
    cast(&mut state, axe, &[]);
    while let Some(decision) = settle(&mut state) {
        match decision {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(first)));
                assert!(legal_targets.contains(&Target::Object(second)));
                act(&mut state, Action::ChooseTarget(Target::Object(first)));
            }
            other => panic!("unexpected choice: {other:?}"),
        }
    }
    assert_eq!(energy(&state, P0), 2);
    assert_eq!(energy(&state, P1), 0);
    assert_eq!(
        state
            .objects
            .get(axe)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(first)
    );
    assert_eq!(power_toughness(&state, first), (3, 1));

    // Energy survives a state round trip.
    state = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(energy(&state, P0), 2);

    // Equip--Pay {E}{E}, as a sorcery.
    assert!(activatable(&mut state, axe, 0));
    act(&mut state, Action::ActivateAbility(axe, 0));
    match next(&mut state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(second)));
            act(&mut state, Action::ChooseTarget(Target::Object(second)));
        }
        other => panic!("expected the equip target, got {other:?}"),
    }
    assert_eq!(energy(&state, P0), 0, "energy is paid on activation");
    settled(&mut state);
    assert_eq!(
        state
            .objects
            .get(axe)
            .v4
            .attached_to
            .map(|link| link.object),
        Some(second)
    );
    assert_eq!(power_toughness(&state, second), (3, 3));
    assert_eq!(power_toughness(&state, first), (1, 1));
    assert!(!activatable(&mut state, axe, 0), "no energy left");
}

#[test]
fn inventors_axe_has_flash() {
    let mut state = islands();
    state.active_player = P1;
    state.priority_player = P0;
    put(&mut state, P0, "Faerie Seer", Zone::Battlefield);
    let axe = put(&mut state, P0, "Inventor's Axe", Zone::Hand);
    let moxite = put(&mut state, P0, "Melded Moxite", Zone::Hand);
    pool(&mut state, P0, &[ManaColor::R], 1);
    let (castable_spells, _) = priority(&mut state);
    assert!(castable_spells.contains(&axe));
    assert!(!castable_spells.contains(&moxite));
}
