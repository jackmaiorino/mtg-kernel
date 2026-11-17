//! Sideboard wave S1o: native rules tests for Stronghold Gambit, Eowyn,
//! Fearless Knight, Unlicensed Hearse and Outland Liberator, plus
//! public-session consumer games with replay verification.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, Supertype, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{DayNightV1, GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use serde_json::Value;
use std::collections::BTreeSet;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const GAMBIT: &str = "Stronghold Gambit";
const EOWYN: &str = "Eowyn, Fearless Knight";
const HEARSE: &str = "Unlicensed Hearse";
const LIBERATOR: &str = "Outland Liberator";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// First main phase with `active` active and holding priority, both libraries
/// twenty Islands.
fn game(active: PlayerId) -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4f00_0001,
    );
    state.active_player = active;
    state.priority_player = active;
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

fn deck(rows: &[(&str, u32)]) -> CustomDeckV1 {
    CustomDeckV1 {
        cards: rows
            .iter()
            .map(|&(name, count)| CustomCardCountV1 {
                name: name.to_owned(),
                count,
            })
            .collect(),
        companion: None,
    }
}

/// The priority holder and the spells and abilities it may cast or activate.
fn priority(state: &mut GameState) -> (PlayerId, Vec<ObjectId>, Vec<(ObjectId, u8)>) {
    match next(state) {
        Decision::CastSpellOrPass {
            player,
            castable_spells,
            activatable_abilities,
            ..
        } => (player, castable_spells, activatable_abilities),
        other => panic!("expected priority, got {other:?}"),
    }
}

/// Casts `spell` with the mana already available to its caster.
fn cast(state: &mut GameState, spell: ObjectId) {
    assert!(
        priority(state).1.contains(&spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
}

fn activatable(state: &mut GameState, source: ObjectId, index: u8) -> bool {
    priority(state).2.contains(&(source, index))
}

/// Activates `source`'s ability `index` with the mana already available.
fn activate(state: &mut GameState, source: ObjectId, index: u8) {
    assert!(
        activatable(state, source, index),
        "{} ability {index} not activatable",
        state.objects.get(source).name
    );
    act(state, Action::ActivateAbility(source, index));
}

/// Answers the pending target prompt with `target`, returning the legal
/// targets it offered.
fn choose_target(state: &mut GameState, target: Target) -> Vec<Target> {
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                legal_targets.contains(&target),
                "{target:?} not in {legal_targets:?}"
            );
            act(state, Action::ChooseTarget(target));
            legal_targets
        }
        other => panic!("expected a target prompt, got {other:?}"),
    }
}

fn power(state: &GameState, object: ObjectId) -> i32 {
    engine::effective_power(state, object)
}

fn toughness(state: &GameState, object: ObjectId) -> i32 {
    engine::effective_toughness(state, object)
}

fn set(targets: &[Target]) -> BTreeSet<ObjectId> {
    targets
        .iter()
        .map(|target| match target {
            Target::Object(object) => *object,
            other => panic!("unexpected target {other:?}"),
        })
        .collect()
}

fn objects(ids: &[ObjectId]) -> BTreeSet<ObjectId> {
    ids.iter().copied().collect()
}

// ---- Card Definitions ---------------------------------------------------------------

#[test]
fn s1o_definitions_have_printed_characteristics() {
    let gambit = def(GAMBIT);
    assert_eq!(gambit.capability, CardCapability::Full);
    assert_eq!(gambit.types, &[CardType::Sorcery]);
    assert_eq!(gambit.colors, &[ManaColor::R]);
    assert_eq!(gambit.mana_value, 2);

    let eowyn = def(EOWYN);
    assert_eq!(eowyn.capability, CardCapability::Full);
    assert_eq!(eowyn.types, &[CardType::Creature]);
    assert_eq!(eowyn.subtypes, &[Subtype::Human, Subtype::Knight]);
    assert_eq!(eowyn.supertypes, &[Supertype::Legendary]);
    assert_eq!(eowyn.colors, &[ManaColor::R, ManaColor::W]);
    assert_eq!(eowyn.mana_value, 4);
    assert_eq!(eowyn.power, Some(3));
    assert_eq!(eowyn.toughness, Some(4));
    assert!(eowyn.keywords.has(Keywords::HASTE));

    let hearse = def(HEARSE);
    assert_eq!(hearse.capability, CardCapability::Full);
    assert_eq!(hearse.types, &[CardType::Artifact]);
    assert_eq!(hearse.subtypes, &[Subtype::Vehicle]);
    assert_eq!(hearse.mana_value, 2);
    assert!(hearse.colors.is_empty());

    let liberator = def(LIBERATOR);
    assert_eq!(liberator.capability, CardCapability::Full);
    assert_eq!(liberator.types, &[CardType::Creature]);
    assert_eq!(liberator.subtypes, &[Subtype::Human, Subtype::Werewolf]);
    assert_eq!(liberator.colors, &[ManaColor::G]);
    assert_eq!(liberator.mana_value, 2);
    assert_eq!(liberator.power, Some(2));
    assert_eq!(liberator.toughness, Some(2));
    let back = liberator
        .transform_face
        .as_ref()
        .expect("Frenzied Trapbreaker");
    assert_eq!(back.name, "Frenzied Trapbreaker");
    assert_eq!(back.types, &[CardType::Creature]);
    assert_eq!(back.subtypes, &[Subtype::Werewolf]);
    assert_eq!(back.colors, &[ManaColor::G]);
    assert_eq!(back.power, Some(3));
    assert_eq!(back.toughness, Some(3));
}

// ---- Stronghold Gambit --------------------------------------------------------------

/// Answers `player`'s Gambit hand prompt with `card`, returning the
/// candidates it offered. The prompt takes exactly one card.
fn gambit_pick(state: &mut GameState, player: PlayerId, card: ObjectId) -> Vec<Target> {
    let legal = match next(state) {
        Decision::ChooseEffectTargets {
            player: chooser,
            legal_targets,
            min_targets,
            max_targets,
            can_finish,
            ..
        } => {
            assert_eq!(chooser, player);
            assert_eq!((min_targets, max_targets), (1, 1));
            assert!(!can_finish, "a card must be chosen");
            legal_targets
        }
        other => panic!("expected the Gambit hand prompt, got {other:?}"),
    };
    assert!(legal.contains(&Target::Object(card)));
    act(state, Action::ChooseEffectTarget(Target::Object(card)));
    if let Decision::ChooseEffectTargets {
        player: chooser,
        can_finish: true,
        selected_count: 1,
        ..
    } = next(state)
    {
        if chooser == player {
            act(state, Action::FinishEffectSelection);
        }
    }
    legal
}

/// Casts Stronghold Gambit for `actor` from a fresh hand card.
fn cast_gambit(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let gambit = put(state, actor, GAMBIT, Zone::Hand);
    pool(state, actor, &[ManaColor::R], 1);
    cast(state, gambit);
    gambit
}

/// "Each player chooses a card in their hand. Then each player reveals their
/// chosen card. The owner of each creature card revealed this way with the
/// lowest mana value puts it onto the battlefield." The active player
/// chooses first; the second chooser is offered only their own hand; a
/// creature card with a higher mana value stays in its owner's hand.
#[test]
fn stronghold_gambit_lowest_creature_enters_and_the_rest_stay_in_hand_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let griselbrand = put(&mut state, actor, "Griselbrand", Zone::Hand);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Hand);
        let rootwalla = put(&mut state, opponent, "Basking Rootwalla", Zone::Hand);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        let gambit = cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());

        let first = gambit_pick(&mut state, actor, griselbrand);
        assert_eq!(set(&first), objects(&[griselbrand, thug]));
        assert_eq!(zone(&state, griselbrand), Zone::Hand);
        let second = gambit_pick(&mut state, opponent, rootwalla);
        assert_eq!(set(&second), objects(&[rootwalla, bolt]));
        settled(&mut state);

        assert_eq!(zone(&state, gambit), Zone::Graveyard);
        assert_eq!(zone(&state, rootwalla), Zone::Battlefield);
        assert_eq!(state.objects.get(rootwalla).controller, opponent);
        assert_eq!(zone(&state, griselbrand), Zone::Hand);
        assert_eq!(zone(&state, thug), Zone::Hand);
        assert_eq!(zone(&state, bolt), Zone::Hand);
    }
}

/// A tie puts both creature cards onto the battlefield at once, each under
/// its owner's control. A Containment Priest entering in that batch does not
/// exile the other entrant (its replacement sees the battlefield before the
/// batch); a Priest already on the battlefield exiles a creature that enters
/// this way, since it was not cast. A noncreature choice and an empty hand
/// put nothing onto the battlefield.
#[test]
fn stronghold_gambit_ties_containment_priest_noncreature_and_empty_hand_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        // Tie at mana value 2, with a Priest among the entrants.
        let mut state = game(actor);
        let priest = put(&mut state, actor, "Containment Priest", Zone::Hand);
        let thug = put(&mut state, opponent, "Golgari Thug", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, priest);
        gambit_pick(&mut state, opponent, thug);
        settled(&mut state);
        assert_eq!(zone(&state, priest), Zone::Battlefield);
        assert_eq!(state.objects.get(priest).controller, actor);
        assert_eq!(zone(&state, thug), Zone::Battlefield);
        assert_eq!(state.objects.get(thug).controller, opponent);

        // A Priest already on the battlefield; the opponent reveals a
        // noncreature card.
        let mut state = game(actor);
        put(
            &mut state,
            opponent,
            "Containment Priest",
            Zone::Battlefield,
        );
        let narcomoeba = put(&mut state, actor, "Narcomoeba", Zone::Hand);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, narcomoeba);
        gambit_pick(&mut state, opponent, bolt);
        settled(&mut state);
        assert_eq!(zone(&state, narcomoeba), Zone::Exile);
        assert_eq!(zone(&state, bolt), Zone::Hand);

        // The opponent's hand is empty: only the caster chooses.
        let mut state = game(actor);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, thug);
        settled(&mut state);
        assert_eq!(zone(&state, thug), Zone::Battlefield);

        // Neither player has a card in hand: the spell does nothing.
        let mut state = game(actor);
        let gambit = cast_gambit(&mut state, actor);
        settled(&mut state);
        assert_eq!(zone(&state, gambit), Zone::Graveyard);
    }
}

/// Phyrexian Metamorph chosen with no artifact or creature on the battlefield
/// has nothing to copy, enters as itself (a 0/0) and dies. With something to
/// copy, its owner's as-enters copy choice is not modeled on this route, so
/// the engine refuses the game rather than guess.
#[test]
fn stronghold_gambit_metamorph_enters_uncopied_or_refuses_its_copy_menu() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let griselbrand = put(&mut state, actor, "Griselbrand", Zone::Hand);
        let metamorph = put(&mut state, opponent, "Phyrexian Metamorph", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, griselbrand);
        gambit_pick(&mut state, opponent, metamorph);
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Graveyard);
        assert_eq!(zone(&state, griselbrand), Zone::Hand);

        let mut state = game(actor);
        put(&mut state, actor, "Golgari Thug", Zone::Battlefield);
        let griselbrand = put(&mut state, actor, "Griselbrand", Zone::Hand);
        let metamorph = put(&mut state, opponent, "Phyrexian Metamorph", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, griselbrand);
        let chosen = Target::Object(metamorph);
        match next(&mut state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&chosen))
            }
            other => panic!("expected the opponent's prompt, got {other:?}"),
        }
        act(&mut state, Action::ChooseEffectTarget(chosen));
        let mut halted = false;
        for _ in 0..20 {
            match engine::advance_until_decision(&mut state) {
                Decision::Halted { .. } => {
                    halted = true;
                    break;
                }
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(&mut state, Action::FinishEffectSelection),
                other => panic!("expected the refusal, got {other:?}"),
            }
        }
        assert!(halted);
        assert_eq!(zone(&state, metamorph), Zone::Hand);
    }
}

/// The second chooser's menu and policy observation are the same whatever the
/// first player picked, and the first pick is not known to the opponent before
/// the reveal. After the reveal, the chosen card that stays in hand is known
/// to its owner's opponent and the one that entered is on the battlefield; the
/// unchosen card stays private.
#[test]
fn stronghold_gambit_second_chooser_learns_nothing_before_the_reveal_in_both_seats() {
    use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
    use mtg_kernel::rl::observe_policy_v5;

    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let run = |pick: usize| {
            let mut state = game(actor);
            mtg_kernel::rl::enable_public_shroud_v1(&mut state);
            let hand = [
                put(&mut state, actor, "Griselbrand", Zone::Hand),
                put(&mut state, actor, "Golgari Thug", Zone::Hand),
            ];
            let rootwalla = put(&mut state, opponent, "Basking Rootwalla", Zone::Hand);
            let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
            cast_gambit(&mut state, actor);
            assert!(settle(&mut state).is_some());
            gambit_pick(&mut state, actor, hand[pick]);
            let menu = format!("{:?}", next(&mut state));
            let observation = format!(
                "{:?}",
                observe_policy_v5(&state, &PolicySurfaceV5::new(), opponent, 0, 0, 0, 1).unwrap()
            );
            assert!(state.known_hand_cards(opponent, actor).is_empty());
            gambit_pick(&mut state, opponent, rootwalla);
            settled(&mut state);
            assert_eq!(zone(&state, rootwalla), Zone::Battlefield);
            assert_eq!(zone(&state, hand[pick]), Zone::Hand);
            let known = state
                .known_hand_cards(opponent, actor)
                .iter()
                .map(|entry| entry.object)
                .collect::<Vec<_>>();
            assert_eq!(known, vec![hand[pick]]);
            assert!(!state
                .known_hand_cards(actor, opponent)
                .iter()
                .any(|entry| entry.object == bolt));
            (menu, observation)
        };
        let (menu_a, observation_a) = run(0);
        let (menu_b, observation_b) = run(1);
        assert_eq!(menu_a, menu_b);
        assert_eq!(observation_a, observation_b);
    }
}

/// Answers the pending Meddling Mage name menu with `name` (its first letter,
/// then the name).
fn choose_mage_name(state: &mut GameState, player: PlayerId, name: &str) {
    let Decision::ChooseMeddlingMageNameV1 {
        player: chooser,
        group: None,
        options,
        ..
    } = next(state)
    else {
        panic!("expected the catalog name groups")
    };
    assert_eq!(chooser, player);
    let letter = name[..1].to_ascii_uppercase();
    let group = options.iter().position(|option| option == &letter).unwrap();
    act(state, Action::ChooseEffectOption(group as u16));
    let Decision::ChooseMeddlingMageNameV1 {
        player: chooser,
        group: Some(_),
        options,
        ..
    } = next(state)
    else {
        panic!("expected the names of the selected group")
    };
    assert_eq!(chooser, player);
    let index = options.iter().position(|option| option == name).unwrap();
    act(state, Action::ChooseEffectOption(index as u16));
}

/// Containment Priest and Meddling Mage (whose name is chosen before it
/// enters, pausing the entry) both enter by Stronghold Gambit, in either
/// order of the batch: the Priest does not exile the Mage, and the Mage's
/// name stops that spell. A Priest already on the battlefield exiles a Mage
/// entering this way.
#[test]
fn stronghold_gambit_priest_and_meddling_mage_enter_together_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for priest_first in [true, false] {
            let mut state = game(actor);
            let (priest_owner, mage_owner) = if priest_first {
                (actor, opponent)
            } else {
                (opponent, actor)
            };
            let priest = put(&mut state, priest_owner, "Containment Priest", Zone::Hand);
            let mage = put(&mut state, mage_owner, "Meddling Mage", Zone::Hand);
            cast_gambit(&mut state, actor);
            assert!(settle(&mut state).is_some());
            let (first, second) = if priest_first {
                (priest, mage)
            } else {
                (mage, priest)
            };
            gambit_pick(&mut state, actor, first);
            gambit_pick(&mut state, opponent, second);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseMeddlingMageNameV1 { player, .. }) if player == mage_owner
            ));
            choose_mage_name(&mut state, mage_owner, "Lightning Bolt");
            settled(&mut state);
            assert_eq!(zone(&state, priest), Zone::Battlefield);
            assert_eq!(zone(&state, mage), Zone::Battlefield);
            state.priority_player = priest_owner;
            let bolt = put(&mut state, priest_owner, "Lightning Bolt", Zone::Hand);
            pool(&mut state, priest_owner, &[ManaColor::R], 0);
            assert!(!priority(&mut state).1.contains(&bolt), "the named spell");
        }

        let mut state = game(actor);
        put(
            &mut state,
            opponent,
            "Containment Priest",
            Zone::Battlefield,
        );
        let mage = put(&mut state, actor, "Meddling Mage", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, mage);
        settled(&mut state);
        assert_eq!(zone(&state, mage), Zone::Exile);
    }
}

/// A Metamorph that a Containment Priest already on the battlefield would
/// exile never enters, so no copy choice arises and the game goes on.
#[test]
fn stronghold_gambit_metamorph_exiled_by_a_priest_needs_no_copy_choice_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        put(&mut state, actor, "Containment Priest", Zone::Battlefield);
        put(&mut state, actor, "Golgari Thug", Zone::Battlefield);
        let griselbrand = put(&mut state, actor, "Griselbrand", Zone::Hand);
        let metamorph = put(&mut state, opponent, "Phyrexian Metamorph", Zone::Hand);
        cast_gambit(&mut state, actor);
        assert!(settle(&mut state).is_some());
        gambit_pick(&mut state, actor, griselbrand);
        gambit_pick(&mut state, opponent, metamorph);
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Exile);
        assert_eq!(zone(&state, griselbrand), Zone::Hand);
    }
}

// ---- Eowyn, Fearless Knight ---------------------------------------------------------

/// Casts Eowyn for `actor` and answers her enters trigger with `target`,
/// returning the targets it offered.
fn cast_eowyn(state: &mut GameState, actor: PlayerId, target: ObjectId) -> (ObjectId, Vec<Target>) {
    let eowyn = put(state, actor, EOWYN, Zone::Hand);
    pool(state, actor, &[ManaColor::R, ManaColor::W], 2);
    cast(state, eowyn);
    let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(state) else {
        panic!("expected Eowyn's trigger target");
    };
    assert!(legal_targets.contains(&Target::Object(target)));
    act(state, Action::ChooseTarget(Target::Object(target)));
    (eowyn, legal_targets)
}

/// Advances until the chosen trigger is on the stack and its controller
/// holds priority.
fn stacked(state: &mut GameState) {
    priority(state);
    assert_eq!(state.stack.len(), 1);
    assert!(state.engine.pending_triggers.is_empty());
}

/// The creatures the opponent's Swords to Plowshares may target.
fn swords_targets(state: &mut GameState, opponent: PlayerId, target: ObjectId) -> Vec<Target> {
    state.priority_player = opponent;
    let swords = put(state, opponent, "Swords to Plowshares", Zone::Hand);
    pool(state, opponent, &[ManaColor::W], 0);
    cast(state, swords);
    let legal = choose_target(state, Target::Object(target));
    settled(state);
    legal
}

/// "When Eowyn enters, exile target creature an opponent controls with
/// greater power. Legendary creatures you control gain protection from each
/// of that creature's colors until end of turn." Only the opponent's
/// creature with power greater than 3 is offered (not lower, not equal, not
/// the caster's own). The multicolored creature's colors give white
/// protection to the legendary creatures, so a white spell can't target
/// them, while a nonlegendary creature and a legendary creature that arrives
/// later remain targetable. Eowyn has haste.
#[test]
fn eowyn_exiles_greater_power_and_protects_legendary_creatures_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let atraxa = put(
            &mut state,
            opponent,
            "Atraxa, Grand Unifier",
            Zone::Battlefield,
        );
        let squee = put(
            &mut state,
            opponent,
            "Squee, Goblin Nabob",
            Zone::Battlefield,
        );
        let thug = put(&mut state, opponent, "Golgari Thug", Zone::Battlefield);
        state.objects.get_mut(thug).counters.plus1_plus1 = 2;
        assert_eq!(power(&state, thug), 3);
        let ragavan = put(
            &mut state,
            actor,
            "Ragavan, Nimble Pilferer",
            Zone::Battlefield,
        );
        let inspector = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let griselbrand = put(&mut state, actor, "Griselbrand", Zone::Battlefield);

        let (eowyn, legal) = cast_eowyn(&mut state, actor, atraxa);
        assert_eq!(legal, vec![Target::Object(atraxa)]);
        assert!(!legal.contains(&Target::Object(squee)));
        assert!(!legal.contains(&Target::Object(griselbrand)));
        settled(&mut state);
        assert_eq!(zone(&state, atraxa), Zone::Exile);
        assert!(engine::has_effective_keyword(
            &state,
            eowyn,
            Keywords::HASTE
        ));

        let later = put(&mut state, actor, "Squee, Goblin Nabob", Zone::Battlefield);
        let legal = swords_targets(&mut state, opponent, inspector);
        assert!(!legal.contains(&Target::Object(eowyn)));
        assert!(!legal.contains(&Target::Object(ragavan)));
        assert!(legal.contains(&Target::Object(later)));
        assert!(legal.contains(&Target::Object(squee)));
        assert_eq!(zone(&state, inspector), Zone::Exile);
    }
}

/// 608.2b: the target's power is compared with Eowyn's again as the trigger
/// resolves. Grown past the target, Eowyn's trigger does nothing and grants
/// no protection. Gone from the battlefield, her last-known power counts:
/// a 3-power Eowyn still exiles the bigger creature, an 8-power one does not.
#[test]
fn eowyn_rechecks_power_on_resolution_with_last_known_power_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        // Eowyn grows to 8 power before the trigger resolves.
        let mut state = game(actor);
        let atraxa = put(
            &mut state,
            opponent,
            "Atraxa, Grand Unifier",
            Zone::Battlefield,
        );
        let ragavan = put(
            &mut state,
            actor,
            "Ragavan, Nimble Pilferer",
            Zone::Battlefield,
        );
        let (eowyn, _) = cast_eowyn(&mut state, actor, atraxa);
        stacked(&mut state);
        state.objects.get_mut(eowyn).counters.plus1_plus1 = 5;
        settled(&mut state);
        assert_eq!(zone(&state, atraxa), Zone::Battlefield);
        let legal = swords_targets(&mut state, opponent, ragavan);
        assert!(legal.contains(&Target::Object(eowyn)));
        assert_eq!(zone(&state, ragavan), Zone::Exile);

        // Eowyn leaves with 3 power: her last-known power is lower.
        let mut state = game(actor);
        let atraxa = put(
            &mut state,
            opponent,
            "Atraxa, Grand Unifier",
            Zone::Battlefield,
        );
        let ragavan = put(
            &mut state,
            actor,
            "Ragavan, Nimble Pilferer",
            Zone::Battlefield,
        );
        let inspector = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let (eowyn, _) = cast_eowyn(&mut state, actor, atraxa);
        stacked(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(eowyn, Zone::Graveyard),
        );
        assert_eq!(zone(&state, eowyn), Zone::Graveyard);
        settled(&mut state);
        assert_eq!(zone(&state, atraxa), Zone::Exile);
        let legal = swords_targets(&mut state, opponent, inspector);
        assert!(!legal.contains(&Target::Object(ragavan)));

        // Eowyn leaves with 8 power.
        let mut state = game(actor);
        let atraxa = put(
            &mut state,
            opponent,
            "Atraxa, Grand Unifier",
            Zone::Battlefield,
        );
        let (eowyn, _) = cast_eowyn(&mut state, actor, atraxa);
        stacked(&mut state);
        state.objects.get_mut(eowyn).counters.plus1_plus1 = 5;
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(eowyn, Zone::Graveyard),
        );
        settled(&mut state);
        assert_eq!(zone(&state, atraxa), Zone::Battlefield);
    }
}

/// The legendary creatures gain protection from exactly the exiled
/// creature's colors: after a mono-black creature is exiled, a black spell
/// cannot target them and a white one can; after a colorless creature is
/// exiled, both can.
#[test]
fn eowyn_grants_protection_from_exactly_the_exiled_colors_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        for (exiled, black_protected) in [("Griselbrand", true), ("Phyrexian Dreadnought", false)] {
            let mut state = game(actor);
            let target = put(&mut state, opponent, exiled, Zone::Battlefield);
            let ragavan = put(
                &mut state,
                actor,
                "Ragavan, Nimble Pilferer",
                Zone::Battlefield,
            );
            let inspector = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
            cast_eowyn(&mut state, actor, target);
            settled(&mut state);
            assert_eq!(zone(&state, target), Zone::Exile);

            state.priority_player = opponent;
            let push = put(&mut state, opponent, "Fatal Push", Zone::Hand);
            pool(&mut state, opponent, &[ManaColor::B], 0);
            cast(&mut state, push);
            let legal = choose_target(&mut state, Target::Object(inspector));
            settled(&mut state);
            assert_eq!(legal.contains(&Target::Object(ragavan)), !black_protected);

            let legal = swords_targets(&mut state, opponent, ragavan);
            assert!(legal.contains(&Target::Object(ragavan)));
            assert_eq!(zone(&state, ragavan), Zone::Exile);
        }
    }
}

// ---- Unlicensed Hearse --------------------------------------------------------------

/// Crews `vehicle` (ability `index`) with `members`.
fn crew(state: &mut GameState, vehicle: ObjectId, index: u8, members: &[ObjectId]) {
    assert!(activatable(state, vehicle, index), "crew not activatable");
    act(state, Action::ActivateAbility(vehicle, index));
    for &member in members {
        match next(state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(member)));
                act(state, Action::ChooseEffectTarget(Target::Object(member)));
            }
            other => panic!("expected a crew choice, got {other:?}"),
        }
    }
    match next(state) {
        Decision::ChooseEffectTargets {
            can_finish: true, ..
        } => act(state, Action::FinishEffectSelection),
        other => panic!("expected the crew to be complete, got {other:?}"),
    }
    settled(state);
}

/// Answers the Hearse's optional target prompt with `picks`, then finishes it
/// if it is still open. Returns the cards offered before each pick.
fn hearse_targets(state: &mut GameState, picks: &[ObjectId]) -> Vec<Vec<Target>> {
    let mut offered = Vec::new();
    for &pick in picks {
        match next(state) {
            Decision::ChooseEffectTargets {
                legal_targets,
                can_finish,
                max_targets,
                ..
            } => {
                assert!(can_finish, "up to two");
                assert_eq!(max_targets, 2);
                assert!(legal_targets.contains(&Target::Object(pick)));
                offered.push(legal_targets);
                act(state, Action::ChooseEffectTarget(Target::Object(pick)));
            }
            other => panic!("expected the Hearse's targets, got {other:?}"),
        }
    }
    if let Decision::ChooseEffectTargets {
        can_finish: true, ..
    } = next(state)
    {
        act(state, Action::FinishEffectSelection);
    }
    offered
}

/// "{T}: Exile up to two target cards from a single graveyard." The second
/// target must come from the first one's graveyard. "Unlicensed Hearse's
/// power and toughness are each equal to the number of cards exiled with
/// it." "Crew 2". Zero targets are legal.
#[test]
fn unlicensed_hearse_exiles_from_one_graveyard_grows_and_crews_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let hearse = put(&mut state, actor, HEARSE, Zone::Battlefield);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Graveyard);
        let seal = put(&mut state, opponent, "Seal of Fire", Zone::Graveyard);
        let island = put(&mut state, actor, "Island", Zone::Graveyard);
        assert_eq!((power(&state, hearse), toughness(&state, hearse)), (0, 0));
        assert!(!engine::object_has_type(&state, hearse, CardType::Creature));

        activate(&mut state, hearse, 0);
        let offered = hearse_targets(&mut state, &[bolt, seal]);
        assert_eq!(set(&offered[0]), objects(&[bolt, seal, island]));
        assert_eq!(set(&offered[1]), objects(&[seal]));
        settled(&mut state);
        assert_eq!(zone(&state, bolt), Zone::Exile);
        assert_eq!(zone(&state, seal), Zone::Exile);
        assert_eq!(zone(&state, island), Zone::Graveyard);
        assert!(state.objects.get(hearse).tapped);
        assert_eq!((power(&state, hearse), toughness(&state, hearse)), (2, 2));

        // Crew 2 with two 1/1s: a 2/2 artifact creature.
        let first = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        let second = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        crew(&mut state, hearse, 1, &[first, second]);
        assert!(engine::object_has_type(&state, hearse, CardType::Creature));
        assert_eq!((power(&state, hearse), toughness(&state, hearse)), (2, 2));

        // No targets at all.
        let mut state = game(actor);
        let hearse = put(&mut state, actor, HEARSE, Zone::Battlefield);
        put(&mut state, opponent, "Lightning Bolt", Zone::Graveyard);
        activate(&mut state, hearse, 0);
        hearse_targets(&mut state, &[]);
        settled(&mut state);
        assert!(state.objects.get(hearse).tapped);
        assert_eq!(power(&state, hearse), 0);
    }
}

/// A crewed Hearse with nothing exiled with it is a 0/0 and dies (state-based
/// action). The {T} ability is usable the turn the Hearse arrived while it is
/// not a creature, and not once it is a crewed creature. A new object (the
/// Hearse blinked) has no cards exiled with it.
#[test]
fn unlicensed_hearse_zero_zero_dies_summoning_sickness_and_blink_reset_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        let mut state = game(actor);
        let hearse = put(&mut state, actor, HEARSE, Zone::Battlefield);
        let first = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        let second = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        crew(&mut state, hearse, 1, &[first, second]);
        assert_eq!(zone(&state, hearse), Zone::Graveyard);

        let mut state = game(actor);
        let hearse = put(&mut state, actor, HEARSE, Zone::Battlefield);
        state.objects.get_mut(hearse).summoning_sick = true;
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Graveyard);
        assert!(activatable(&mut state, hearse, 0));
        activate(&mut state, hearse, 0);
        hearse_targets(&mut state, &[bolt]);
        settled(&mut state);
        assert_eq!(power(&state, hearse), 1);
        state.objects.get_mut(hearse).tapped = false;
        let first = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        let second = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        crew(&mut state, hearse, 1, &[first, second]);
        assert_eq!(zone(&state, hearse), Zone::Battlefield);
        assert_eq!((power(&state, hearse), toughness(&state, hearse)), (1, 1));
        assert!(!activatable(&mut state, hearse, 0));

        // Blinked: the returned Hearse is a new object with nothing exiled
        // with it, though the card stays in exile.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(hearse, Zone::Exile));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(hearse, Zone::Battlefield),
        );
        assert_eq!(zone(&state, hearse), Zone::Battlefield);
        assert_eq!(zone(&state, bolt), Zone::Exile);
        assert_eq!(power(&state, hearse), 0);
    }
}

/// A Phyrexian Metamorph that entered as a copy of Unlicensed Hearse has the
/// Hearse's abilities, and its power and toughness count the cards exiled
/// with the copy itself.
#[test]
fn phyrexian_metamorph_copying_unlicensed_hearse_counts_its_own_exiled_cards() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let hearse = put(&mut state, opponent, HEARSE, Zone::Battlefield);
        let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Graveyard);
        let seal = put(&mut state, opponent, "Seal of Fire", Zone::Graveyard);
        let metamorph = put(&mut state, actor, "Phyrexian Metamorph", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 3);
        cast(&mut state, metamorph);
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert!(legal_targets.contains(&Target::Object(hearse)));
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(hearse)),
                );
            }
            other => panic!("expected the copy choice, got {other:?}"),
        }
        if let Some(Decision::ChooseEffectTargets {
            can_finish: true, ..
        }) = settle(&mut state)
        {
            act(&mut state, Action::FinishEffectSelection);
        }
        settled(&mut state);
        assert_eq!(zone(&state, metamorph), Zone::Battlefield);
        assert_eq!(engine::effective_name(&state, metamorph), HEARSE);
        assert_eq!(power(&state, metamorph), 0);

        activate(&mut state, metamorph, 0);
        hearse_targets(&mut state, &[bolt, seal]);
        settled(&mut state);
        assert_eq!(zone(&state, bolt), Zone::Exile);
        assert_eq!(zone(&state, seal), Zone::Exile);
        assert_eq!(power(&state, hearse), 0);
        let first = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        let second = put(&mut state, actor, "Basking Rootwalla", Zone::Battlefield);
        crew(&mut state, metamorph, 1, &[first, second]);
        assert!(engine::object_has_type(
            &state,
            metamorph,
            CardType::Creature
        ));
        assert_eq!(
            (power(&state, metamorph), toughness(&state, metamorph)),
            (2, 2)
        );
    }
}

// ---- Outland Liberator --------------------------------------------------------------

/// Passes priority (declaring no attackers or blockers) until `until`.
fn pass_until(state: &mut GameState, until: impl Fn(&GameState) -> bool) {
    for _ in 0..500 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if until(state) => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(vec![])),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(vec![])),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }
    panic!("condition never reached");
}

/// "{1}, Sacrifice this creature: Destroy target artifact or enchantment."
#[test]
fn outland_liberator_sacrifices_to_destroy_an_artifact_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let liberator = put(&mut state, actor, LIBERATOR, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        pool(&mut state, actor, &[], 1);
        activate(&mut state, liberator, 0);
        choose_target(&mut state, Target::Object(ring));
        settled(&mut state);
        assert_eq!(zone(&state, liberator), Zone::Graveyard);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
    }
}

/// Daybound: a Liberator makes it day; a turn with no spells makes it night
/// and it becomes Frenzied Trapbreaker, a 3/3 whose attack trigger destroys
/// an artifact or enchantment the defending player controls (not one its
/// controller controls) and which keeps the sacrifice ability. A turn with
/// two spells makes it day again. A Liberator cast at night enters as
/// Frenzied Trapbreaker.
#[test]
fn outland_liberator_turns_into_frenzied_trapbreaker_at_night_and_back_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let liberator = put(&mut state, actor, LIBERATOR, Zone::Battlefield);
        let own_ring = put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let seal = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        mtg_kernel::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.day_night_v1, Some(DayNightV1::Day));
        assert_eq!(state.objects.get(liberator).v4.face_index, 0);
        assert_eq!(power(&state, liberator), 2);

        // The actor casts nothing: night as the opponent's turn begins.
        pass_until(&mut state, |s| {
            s.active_player == opponent && s.step == Step::Main1
        });
        assert_eq!(state.day_night_v1, Some(DayNightV1::Night));
        assert_eq!(state.objects.get(liberator).v4.face_index, 1);
        assert_eq!(state.objects.get(liberator).name, "Frenzied Trapbreaker");
        assert_eq!(
            (power(&state, liberator), toughness(&state, liberator)),
            (3, 3)
        );

        // The opponent casts nothing either; on the actor's turn the
        // Trapbreaker attacks.
        let mut attacked = false;
        for _ in 0..500 {
            match next(&mut state) {
                Decision::DeclareAttackers { player, eligible } if player == actor => {
                    assert!(eligible.contains(&liberator));
                    act(&mut state, Action::DeclareAttackers(vec![liberator]));
                    attacked = true;
                    break;
                }
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(vec![]))
                }
                Decision::DeclareBlockers { .. } => {
                    act(&mut state, Action::DeclareBlockers(vec![]))
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                other => panic!("unexpected decision: {other:?}"),
            }
        }
        assert!(attacked);
        assert_eq!(state.day_night_v1, Some(DayNightV1::Night));
        let Some(Decision::ChooseTargets { legal_targets, .. }) = settle(&mut state) else {
            panic!("expected the attack trigger's target");
        };
        assert_eq!(set(&legal_targets), objects(&[ring, seal]));
        assert!(!legal_targets.contains(&Target::Object(own_ring)));
        act(&mut state, Action::ChooseTarget(Target::Object(ring)));
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Graveyard);

        // The back face keeps "{1}, Sacrifice this creature: Destroy target
        // artifact or enchantment."
        let mut copy = state.clone();
        pool(&mut copy, actor, &[], 1);
        activate(&mut copy, liberator, 0);
        choose_target(&mut copy, Target::Object(seal));
        settled(&mut copy);
        assert_eq!(zone(&copy, seal), Zone::Graveyard);
        assert_eq!(zone(&copy, liberator), Zone::Graveyard);

        // The opponent casts two spells on their turn: day again as the
        // actor's next turn begins.
        pass_until(&mut state, |s| {
            s.active_player == opponent && s.step == Step::Main1
        });
        for _ in 0..2 {
            let bolt = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
            pool(&mut state, opponent, &[ManaColor::R], 0);
            cast(&mut state, bolt);
            choose_target(&mut state, Target::Player(actor));
            settled(&mut state);
        }
        pass_until(&mut state, |s| {
            s.active_player == actor && s.step == Step::Main1
        });
        assert_eq!(state.day_night_v1, Some(DayNightV1::Day));
        assert_eq!(state.objects.get(liberator).v4.face_index, 0);
        assert_eq!(power(&state, liberator), 2);

        // Cast at night, it enters as Frenzied Trapbreaker.
        let mut state = game(actor);
        state.day_night_v1 = Some(DayNightV1::Night);
        let liberator = put(&mut state, actor, LIBERATOR, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::G], 1);
        cast(&mut state, liberator);
        settled(&mut state);
        assert_eq!(zone(&state, liberator), Zone::Battlefield);
        assert_eq!(state.objects.get(liberator).v4.face_index, 1);
        assert_eq!(power(&state, liberator), 3);
    }
}

/// By day the front face, Outland Liberator, attacks with no trigger: only
/// Frenzied Trapbreaker has "Whenever this creature attacks".
#[test]
fn outland_liberator_front_face_attacks_without_a_trigger_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let liberator = put(&mut state, actor, LIBERATOR, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        mtg_kernel::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.day_night_v1, Some(DayNightV1::Day));
        assert_eq!(state.objects.get(liberator).v4.face_index, 0);
        let mut attacked = false;
        for _ in 0..200 {
            match next(&mut state) {
                Decision::DeclareAttackers { player, eligible } if player == actor && !attacked => {
                    assert!(eligible.contains(&liberator));
                    act(&mut state, Action::DeclareAttackers(vec![liberator]));
                    attacked = true;
                }
                Decision::CastSpellOrPass { .. } if attacked && state.step == Step::Main2 => break,
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::DeclareBlockers { .. } => {
                    act(&mut state, Action::DeclareBlockers(vec![]))
                }
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                other => panic!("no trigger expected, got {other:?}"),
            }
        }
        assert!(attacked);
        assert_eq!(state.step, Step::Main2);
        assert_eq!(state.objects.get(liberator).v4.face_index, 0);
        assert_eq!(zone(&state, ring), Zone::Battlefield);
    }
}

// ---- S1o Public-Session Consumer Games ----------------------------------------------

#[derive(Default)]
struct S1oSessionEvents {
    gambit_picks: usize,
    hearse_activations: usize,
    liberator_activations: usize,
}

impl S1oSessionEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let card = semantic["source"]["card_db_id"]
            .as_u64()
            .map(|id| CARD_DEFS[id as usize].name);
        match semantic["action_kind"].as_str() {
            Some("activate_ability") if card == Some(HEARSE) => self.hearse_activations += 1,
            Some("activate_ability") if card == Some(LIBERATOR) => self.liberator_activations += 1,
            Some(kind) if kind.starts_with("choose") && card == Some(GAMBIT) => {
                self.gambit_picks += 1
            }
            _ => {}
        }
    }
}

#[test]
fn s1o_decks_finish_and_replay_through_public_sessions() {
    let boros = deck(&[
        ("Plains", 9),
        ("Mountain", 9),
        (EOWYN, 6),
        (GAMBIT, 6),
        ("Thalia, Guardian of Thraben", 5),
        ("Squee, Goblin Nabob", 5),
    ]);
    let green = deck(&[
        ("Forest", 14),
        ("Sol Ring", 4),
        (LIBERATOR, 8),
        (HEARSE, 6),
        ("Basking Rootwalla", 8),
    ]);

    let mut totals = S1oSessionEvents::default();
    let tracked = [EOWYN, GAMBIT, LIBERATOR, HEARSE];
    let mut cast_totals = [0usize; 4];

    for (pair, first_seed) in [
        ([boros.clone(), green.clone()], 0x5331_4f00_u64),
        ([green, boros], 0x5331_4f10),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                let mut events = S1oSessionEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                for (slot, name) in cast_totals.iter_mut().zip(tracked) {
                    *slot += game
                        .cast_cards
                        .iter()
                        .map(|cards| cards.get(&id(name)).copied().unwrap_or(0))
                        .sum::<usize>();
                }
                totals.gambit_picks += events.gambit_picks;
                totals.hearse_activations += events.hearse_activations;
                totals.liberator_activations += events.liberator_activations;
            }
        }
    }

    assert!(cast_totals[0] > 0, "no Eowyn cast");
    assert!(cast_totals[1] > 0, "no Stronghold Gambit cast");
    assert!(cast_totals[2] > 0, "no Outland Liberator cast");
    assert!(cast_totals[3] > 0, "no Unlicensed Hearse cast");
    assert!(totals.gambit_picks > 0, "no Stronghold Gambit hand choice");
    assert!(
        totals.liberator_activations > 0,
        "no Outland Liberator activation"
    );
    assert!(
        totals.hearse_activations > 0,
        "no Unlicensed Hearse activation"
    );
}
