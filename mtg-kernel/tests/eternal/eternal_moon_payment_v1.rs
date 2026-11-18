//! The automatic payment planner for a permanent whose printed abilities an
//! effect removed and that has granted mana abilities instead (Blood Moon,
//! Harbinger of the Seas, Spreading Seas, Alpine Moon): it plans the
//! permanent only for what tapping it by hand adds, one mana a tap (305.7).
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_sideboard_s1g_v1::{alpine_name_options, alpine_option_name};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{self, ManaColor};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const COLORS: [ManaColor; 6] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
    ManaColor::C,
];

fn opponent_of(player: PlayerId) -> PlayerId {
    if player == P0 {
        P1
    } else {
        P0
    }
}

/// A first main phase of `actor`, who holds priority. Each library holds
/// twenty Islands, and both hands and battlefields are empty.
fn game(actor: PlayerId) -> GameState {
    let ids = vec![card_id_by_name("Island").unwrap(); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4d4f_4f4e,
    );
    state.active_player = actor;
    state.priority_player = actor;
    state.step = Step::Main1;
    state
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

fn mana(state: &GameState, player: PlayerId, color: ManaColor) -> u8 {
    state.players[player.index()].mana_pool[color.pool_index()]
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

/// `settle`, answering every target, card selection and option with its
/// first legal answer (Brainstorm's cards put back, Thought-Knot Seer's
/// target).
fn settled_choosing_first(state: &mut GameState) {
    while let Some(decision) = settle(state) {
        match decision {
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => act(state, Action::FinishEffectSelection),
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                act(state, Action::ChooseEffectTarget(legal_targets[0]))
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                act(state, Action::ChooseTarget(legal_targets[0]))
            }
            Decision::ChooseEffectOption { .. } => act(state, Action::ChooseEffectOption(0)),
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}

fn castable(state: &mut GameState, spell: ObjectId) -> bool {
    match next(state) {
        Decision::CastSpellOrPass {
            castable_spells, ..
        } => castable_spells.contains(&spell),
        other => panic!("expected priority, got {other:?}"),
    }
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
                assert!(legal_targets.contains(target), "{target:?}");
                act(state, Action::ChooseTarget(*target));
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
}

/// Casts `name` from `player`'s hand with exactly its cost in the pool, so
/// no permanent pays for it, and resolves it.
fn cast_from_pool(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    colored: &[ManaColor],
    colorless: u8,
    targets: &[Target],
) -> ObjectId {
    let card = put(state, player, name, Zone::Hand);
    pool(state, player, colored, colorless);
    cast(state, card, targets);
    settled(state);
    assert_eq!(state.objects.get(card).zone, Zone::Battlefield, "{name}");
    assert_eq!(state.players[player.index()].mana_pool, [0; 6], "{name}");
    card
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

/// How much `color` tapping `source` by hand adds, tried on a copy, or
/// `None` when tapping it for `color` is refused. A source with a single
/// mana choice takes the plain activation.
fn tapped_for(state: &GameState, source: ObjectId, color: ManaColor) -> Option<u8> {
    let player = state.priority_player;
    let before = mana(state, player, color);
    [
        Action::ActivateManaAbilityChoice(source, color),
        Action::ActivateManaAbility(source),
    ]
    .into_iter()
    .find_map(|action| {
        let mut tapped = state.clone();
        engine::step(&mut tapped, action).ok()?;
        let added = mana(&tapped, player, color).checked_sub(before)?;
        (added > 0).then_some(added)
    })
}

/// The planner's choices for `source`, if it plans `source` at all.
fn planned(state: &GameState, player: PlayerId, source: ObjectId) -> Option<mana::ManaSource> {
    mana::gather_sources(player, state)
        .into_iter()
        .find(|planned| planned.id == source)
}

fn sorted(colors: &[ManaColor]) -> Vec<ManaColor> {
    COLORS
        .into_iter()
        .filter(|color| colors.contains(color))
        .collect()
}

/// An Urza's Saga that `actor` played on this turn, at `actor`'s next
/// precombat main phase once its chapter II ability resolved.
fn saga_after_chapter_two(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let opponent = opponent_of(actor);
    let saga = put(state, actor, "Urza's Saga", Zone::Hand);
    next(state);
    act(state, Action::PlayLand(saga));
    settled(state);
    assert_eq!(state.objects.get(saga).counters.lore, 1);
    pass_until(state, |state| {
        state.active_player == opponent && state.step == Step::Main1
    });
    pass_until(state, |state| {
        state.active_player == actor && state.step == Step::Main1 && state.stack.is_empty()
    });
    assert_eq!(state.objects.get(saga).counters.lore, 2);
    saga
}

fn alpine_option(land: &str) -> u16 {
    (0..alpine_name_options().len())
        .map(|option| u16::try_from(option).unwrap())
        .find(|&option| alpine_option_name(option) == Some(land))
        .unwrap_or_else(|| panic!("{land} is not a nonbasic land name"))
}

/// Puts an Alpine Moon from `player`'s hand onto the battlefield, naming
/// `land`, and settles the stack.
fn alpine_moon_naming(state: &mut GameState, player: PlayerId, land: &str) {
    let moon = put(state, player, "Alpine Moon", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(moon, Zone::Battlefield));
    match settle(state) {
        Some(Decision::ChooseEffectOption {
            player: chooser,
            source,
            ..
        }) => {
            assert_eq!((chooser, source), (player, moon));
            act(state, Action::ChooseEffectOption(alpine_option(land)));
        }
        other => panic!("expected the name choice, got {other:?}"),
    }
    settled(state);
}

// ---- Scenarios ------------------------------------------------------------------------

/// A Volcanic Island of `actor`'s under a Blood Moon `actor` cast.
fn volcanic_under_blood_moon(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    cast_from_pool(&mut state, actor, "Blood Moon", &[ManaColor::R], 2, &[]);
    assert!(!state.objects.get(volcanic).tapped);
    (state, volcanic)
}

/// A Volcanic Island of `actor`'s under a Harbinger of the Seas `actor` cast.
fn volcanic_under_harbinger(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    cast_from_pool(
        &mut state,
        actor,
        "Harbinger of the Seas",
        &[ManaColor::U, ManaColor::U],
        1,
        &[],
    );
    assert!(!state.objects.get(volcanic).tapped);
    (state, volcanic)
}

/// A Volcanic Island of `actor`'s enchanted by `actor`'s Spreading Seas.
fn volcanic_under_spreading_seas(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    cast_from_pool(
        &mut state,
        actor,
        "Spreading Seas",
        &[ManaColor::U],
        1,
        &[Target::Object(volcanic)],
    );
    assert!(!state.objects.get(volcanic).tapped);
    (state, volcanic)
}

/// `actor`'s Urza's Tower, Mine and Power Plant, under a Blood Moon `actor`
/// cast when `moon`.
fn urza_lands(actor: PlayerId, moon: bool) -> (GameState, [ObjectId; 3]) {
    let mut state = game(actor);
    let lands = [
        put(&mut state, actor, "Urza's Tower", Zone::Battlefield),
        put(&mut state, actor, "Urza's Mine", Zone::Battlefield),
        put(&mut state, actor, "Urza's Power Plant", Zone::Battlefield),
    ];
    if moon {
        cast_from_pool(&mut state, actor, "Blood Moon", &[ManaColor::R], 2, &[]);
    } else {
        settled(&mut state);
    }
    (state, lands)
}

/// `actor`'s Urza's Tower, Mine and Power Plant, with the opponent's Alpine
/// Moon naming Urza's Tower.
fn urza_lands_under_alpine_moon(actor: PlayerId) -> (GameState, [ObjectId; 3]) {
    let mut state = game(actor);
    let lands = [
        put(&mut state, actor, "Urza's Tower", Zone::Battlefield),
        put(&mut state, actor, "Urza's Mine", Zone::Battlefield),
        put(&mut state, actor, "Urza's Power Plant", Zone::Battlefield),
    ];
    alpine_moon_naming(&mut state, opponent_of(actor), "Urza's Tower");
    (state, lands)
}

/// `actor`'s Urza's Saga after chapter II, then the opponent's Alpine Moon
/// naming it, which removes both chapter grants (613.7).
fn saga_under_later_alpine_moon(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let saga = saga_after_chapter_two(&mut state, actor);
    alpine_moon_naming(&mut state, opponent_of(actor), "Urza's Saga");
    (state, saga)
}

/// `actor`'s Urza's Saga that reached no chapter, under a Blood Moon `actor`
/// cast: a Mountain that taps for {R} only.
fn saga_without_chapters_under_blood_moon(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let saga = put(&mut state, actor, "Urza's Saga", Zone::Battlefield);
    cast_from_pool(&mut state, actor, "Blood Moon", &[ManaColor::R], 2, &[]);
    assert_eq!(state.objects.get(saga).counters.lore, 0);
    (state, saga)
}

/// `actor`'s Volcanic Island beside `actor`'s Urborg, Tomb of Yawgmoth and
/// Yavimaya, Cradle of Growth, which add Swamp and Forest to it while its
/// printed abilities stay.
fn volcanic_with_urborg_and_yavimaya(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    put(
        &mut state,
        actor,
        "Urborg, Tomb of Yawgmoth",
        Zone::Battlefield,
    );
    put(
        &mut state,
        actor,
        "Yavimaya, Cradle of Growth",
        Zone::Battlefield,
    );
    settled(&mut state);
    (state, volcanic)
}

/// `actor`'s Volcanic Island with no effect on it.
fn plain_volcanic(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    settled(&mut state);
    (state, volcanic)
}

// ---- Tests ----------------------------------------------------------------------------

/// Under Blood Moon a Volcanic Island is a Mountain with "{T}: Add {R}." only
/// (305.7): it pays for Lightning Bolt and not for Brainstorm. Under
/// Harbinger of the Seas it is an Island: the other way round. Magus of the
/// Moon shares Blood Moon's path (`moon_timestamp`).
#[test]
fn a_dual_land_under_a_land_type_setter_pays_only_the_setters_color_both_seats() {
    for actor in [P0, P1] {
        let opponent = opponent_of(actor);

        let (mut state, volcanic) = volcanic_under_blood_moon(actor);
        let brainstorm = put(&mut state, actor, "Brainstorm", Zone::Hand);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        assert!(!castable(&mut state, brainstorm), "Blood Moon: no {{U}}");
        cast(&mut state, bolt, &[Target::Player(opponent)]);
        settled(&mut state);
        assert!(state.objects.get(volcanic).tapped);
        assert_eq!(state.objects.get(bolt).zone, Zone::Graveyard);
        assert_eq!(state.players[opponent.index()].life, 17);

        let (mut state, volcanic) = volcanic_under_harbinger(actor);
        let brainstorm = put(&mut state, actor, "Brainstorm", Zone::Hand);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        assert!(!castable(&mut state, bolt), "Harbinger: no {{R}}");
        let hand = state.players[actor.index()].hand.len();
        cast(&mut state, brainstorm, &[]);
        settled_choosing_first(&mut state);
        assert!(state.objects.get(volcanic).tapped);
        assert_eq!(state.objects.get(brainstorm).zone, Zone::Graveyard);
        // Brainstorm left the hand, three were drawn and two put back.
        assert_eq!(state.players[actor.index()].hand.len(), hand);
    }
}

/// Under Blood Moon the Urza lands are Mountains: each taps for one {R}, so
/// three of them pay three mana and no {C}. Without the Moon they pay seven.
#[test]
fn urza_lands_under_blood_moon_pay_one_red_each_both_seats() {
    for actor in [P0, P1] {
        let (mut state, _) = urza_lands(actor, false);
        let sword = put(&mut state, actor, "Meteor Sword", Zone::Hand);
        assert!(castable(&mut state, sword), "Tron pays seven");

        let (mut state, lands) = urza_lands(actor, true);
        let forge = put(&mut state, actor, "Mystic Forge", Zone::Hand);
        let seer = put(&mut state, actor, "Thought-Knot Seer", Zone::Hand);
        let song = put(&mut state, actor, "Seething Song", Zone::Hand);
        assert!(!castable(&mut state, forge), "three mana, not four");
        pool(&mut state, actor, &[ManaColor::R], 0);
        assert!(!castable(&mut state, seer), "no {{C}} under the Moon");
        state.players[actor.index()].mana_pool = [0; 6];
        cast(&mut state, song, &[]);
        settled(&mut state);
        assert!(lands.iter().all(|&land| state.objects.get(land).tapped));
    }
}

/// An Urza's Saga whose chapter grants an Alpine Moon removed has only "{T}:
/// Add one mana of any color.": a {C} pip is not paid from it, a {R} pip is.
#[test]
fn urzas_saga_under_a_later_alpine_moon_pays_no_colorless_pip_both_seats() {
    for actor in [P0, P1] {
        let opponent = opponent_of(actor);
        let (mut state, saga) = saga_under_later_alpine_moon(actor);
        let seer = put(&mut state, actor, "Thought-Knot Seer", Zone::Hand);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        pool(
            &mut state,
            actor,
            &[ManaColor::U, ManaColor::U, ManaColor::U],
            0,
        );
        assert!(!castable(&mut state, seer), "no {{C}}");
        cast(&mut state, bolt, &[Target::Player(opponent)]);
        settled(&mut state);
        assert!(state.objects.get(saga).tapped);
        assert_eq!(state.players[opponent.index()].life, 17);
    }
}

/// An Urza's Saga that reached no chapter is, under Blood Moon, a Mountain
/// that taps for {R}: it pays for Lightning Bolt.
#[test]
fn urzas_saga_without_chapters_under_blood_moon_pays_red_both_seats() {
    for actor in [P0, P1] {
        let opponent = opponent_of(actor);
        let (mut state, saga) = saga_without_chapters_under_blood_moon(actor);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        cast(&mut state, bolt, &[Target::Player(opponent)]);
        settled(&mut state);
        assert!(state.objects.get(saga).tapped);
        assert_eq!(state.players[opponent.index()].life, 17);
    }
}

/// Spreading Seas: "Enchanted land is an Island." A Volcanic Island it
/// enchants pays {U} and not {R}.
#[test]
fn a_dual_land_under_spreading_seas_pays_only_blue_both_seats() {
    for actor in [P0, P1] {
        let (mut state, volcanic) = volcanic_under_spreading_seas(actor);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let brainstorm = put(&mut state, actor, "Brainstorm", Zone::Hand);
        assert!(!castable(&mut state, bolt), "no {{R}}");
        cast(&mut state, brainstorm, &[]);
        settled_choosing_first(&mut state);
        assert!(state.objects.get(volcanic).tapped);
    }
}

/// A land an Alpine Moon names has "{T}: Add one mana of any color." only: an
/// Urza's Tower it names is planned for one mana of any color, not for its
/// printed three {C}.
#[test]
fn a_land_alpine_moon_names_is_planned_for_one_mana_of_any_color_both_seats() {
    for actor in [P0, P1] {
        let (state, [tower, _, _]) = urza_lands_under_alpine_moon(actor);
        let source = planned(&state, actor, tower).expect("the Tower is a source");
        assert_eq!(source.yield_per_tap, 1);
        assert_eq!(
            sorted(&source.choices),
            vec![
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G
            ]
        );
    }
}

/// In every scenario of this module the planner and tapping by hand agree:
/// each color the planner lists for an untapped source is one tapping it by
/// hand adds, in the amount it plans, and each color tapping an untapped
/// permanent by hand adds is one the planner lists for it.
#[test]
fn the_planner_lists_exactly_what_tapping_by_hand_adds_both_seats() {
    for actor in [P0, P1] {
        let states = [
            volcanic_under_blood_moon(actor).0,
            volcanic_under_harbinger(actor).0,
            volcanic_under_spreading_seas(actor).0,
            urza_lands(actor, false).0,
            urza_lands(actor, true).0,
            urza_lands_under_alpine_moon(actor).0,
            saga_under_later_alpine_moon(actor).0,
            saga_without_chapters_under_blood_moon(actor).0,
            volcanic_with_urborg_and_yavimaya(actor).0,
            plain_volcanic(actor).0,
            volcanic_with_growth_under(actor, "Blood Moon").0,
            volcanic_with_growth_under(actor, "Spreading Seas").0,
            tower_named_by_hamlet_under_spreading_seas(actor).0,
            volcanic_under_spreading_seas_then_compass(actor).0,
            volcanic_under_compass_then_spreading_seas(actor).0,
            volcanic_under_spreading_seas_with_urborg(actor, true).0,
            volcanic_under_spreading_seas_with_urborg(actor, false).0,
            dryad_arbor_under_dress_down_then(actor, "Yavimaya, Cradle of Growth").0,
            dryad_arbor_under_dress_down_then(actor, "Navigator's Compass").0,
            dryad_arbor_under_dress_down_then(actor, "Abundant Growth").0,
        ];
        for (scenario, mut state) in states.into_iter().enumerate() {
            next(&mut state);
            let sources = mana::gather_sources(actor, &state);
            assert!(!sources.is_empty(), "scenario {scenario}");
            for source in &sources {
                for &color in &source.choices {
                    assert_eq!(
                        tapped_for(&state, source.id, color),
                        Some(source.yield_per_tap),
                        "scenario {scenario}: {} for {color:?}",
                        state.objects.get(source.id).name
                    );
                }
            }
            for &permanent in &state.players[actor.index()].battlefield {
                if state.objects.get(permanent).tapped {
                    continue;
                }
                for color in COLORS {
                    let Some(amount) = tapped_for(&state, permanent, color) else {
                        continue;
                    };
                    let source = sources.iter().find(|source| source.id == permanent);
                    assert!(
                        source.is_some_and(|source| {
                            source.choices.contains(&color) && source.yield_per_tap == amount
                        }),
                        "scenario {scenario}: {} taps for {amount} {color:?}, planned {source:?}",
                        state.objects.get(permanent).name
                    );
                }
            }
        }
    }
}

/// The colors tapping `source` by hand adds, in WUBRG then {C} order.
fn tapped_colors(state: &GameState, source: ObjectId) -> Vec<ManaColor> {
    COLORS
        .into_iter()
        .filter(|&color| tapped_for(state, source, color).is_some())
        .collect()
}

/// The colors the planner lists for `source`, in WUBRG then {C} order.
fn planned_colors(state: &GameState, player: PlayerId, source: ObjectId) -> Vec<ManaColor> {
    planned(state, player, source).map_or_else(Vec::new, |planned| sorted(&planned.choices))
}

/// Asserts that tapping `source` by hand and the planner both offer exactly
/// `expected`.
fn assert_offers(state: &mut GameState, source: ObjectId, expected: &[ManaColor], what: &str) {
    next(state);
    let player = state.priority_player;
    assert_eq!(tapped_colors(state, source), expected, "{what}: by hand");
    assert_eq!(
        planned_colors(state, player, source),
        expected,
        "{what}: planner"
    );
}

// ---- Grants beside a removal of printed abilities -------------------------------------

/// `actor`'s Volcanic Island enchanted by `actor`'s Abundant Growth, then
/// `setter` (Blood Moon, or Spreading Seas on the Volcanic Island).
fn volcanic_with_growth_under(actor: PlayerId, setter: &str) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    let growth = put(&mut state, actor, "Abundant Growth", Zone::Hand);
    pool(&mut state, actor, &[ManaColor::G], 0);
    cast(&mut state, growth, &[Target::Object(volcanic)]);
    settled(&mut state);
    assert_eq!(state.objects.get(growth).zone, Zone::Battlefield);
    if setter == "Blood Moon" {
        cast_from_pool(&mut state, actor, setter, &[ManaColor::R], 2, &[]);
    } else {
        cast_from_pool(
            &mut state,
            actor,
            setter,
            &[ManaColor::U],
            1,
            &[Target::Object(volcanic)],
        );
    }
    assert!(!state.objects.get(volcanic).tapped);
    (state, volcanic)
}

/// `actor`'s Urza's Tower enchanted by `actor`'s Spreading Seas, while
/// `actor`'s tapped Petrified Hamlet names Urza's Tower.
fn tower_named_by_hamlet_under_spreading_seas(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let tower = put(&mut state, actor, "Urza's Tower", Zone::Battlefield);
    let hamlet = put(&mut state, actor, "Petrified Hamlet", Zone::Hand);
    next(&mut state);
    act(&mut state, Action::PlayLand(hamlet));
    let tower_option = mtg_kernel::eternal_legacy_e5b_v1::hamlet_name_options()
        .iter()
        .position(|&card_def| card_def == card_id_by_name("Urza's Tower").unwrap())
        .unwrap();
    match settle(&mut state) {
        Some(Decision::ChooseEffectOption { source, .. }) => {
            assert_eq!(source, hamlet);
            act(
                &mut state,
                Action::ChooseEffectOption(u16::try_from(tower_option).unwrap()),
            );
        }
        other => panic!("expected the name choice, got {other:?}"),
    }
    settled(&mut state);
    state.objects.get_mut(hamlet).tapped = true;
    cast_from_pool(
        &mut state,
        actor,
        "Spreading Seas",
        &[ManaColor::U],
        1,
        &[Target::Object(tower)],
    );
    assert!(!state.objects.get(tower).tapped);
    (state, tower)
}

/// Activates `actor`'s new Navigator's Compass on `land`, choosing Mountain.
fn compass_makes_a_mountain(state: &mut GameState, actor: PlayerId, land: ObjectId) {
    compass_adds(state, actor, land, 3);
}

/// Activates `actor`'s new Navigator's Compass on `land`, choosing land type
/// option `option` (Plains, Island, Swamp, Mountain, Forest).
fn compass_adds(state: &mut GameState, actor: PlayerId, land: ObjectId, option: u16) {
    let compass = put(state, actor, "Navigator's Compass", Zone::Battlefield);
    next(state);
    act(state, Action::ActivateAbility(compass, 0));
    match next(state) {
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(legal_targets.contains(&Target::Object(land)));
            act(state, Action::ChooseTarget(Target::Object(land)));
        }
        other => panic!("expected targets, got {other:?}"),
    }
    match settle(state) {
        Some(Decision::ChooseEffectOption { source, .. }) => {
            assert_eq!(source, compass);
            act(state, Action::ChooseEffectOption(option));
        }
        other => panic!("expected the land type choice, got {other:?}"),
    }
    settled(state);
}

/// `actor`'s Volcanic Island under `actor`'s Spreading Seas, then made a
/// Mountain in addition by `actor`'s Navigator's Compass.
fn volcanic_under_spreading_seas_then_compass(actor: PlayerId) -> (GameState, ObjectId) {
    let (mut state, volcanic) = volcanic_under_spreading_seas(actor);
    compass_makes_a_mountain(&mut state, actor, volcanic);
    (state, volcanic)
}

/// `actor`'s Volcanic Island made a Mountain in addition by `actor`'s
/// Navigator's Compass, then enchanted by `actor`'s Spreading Seas.
fn volcanic_under_compass_then_spreading_seas(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    compass_makes_a_mountain(&mut state, actor, volcanic);
    cast_from_pool(
        &mut state,
        actor,
        "Spreading Seas",
        &[ManaColor::U],
        1,
        &[Target::Object(volcanic)],
    );
    (state, volcanic)
}

/// `actor`'s Volcanic Island under `actor`'s Spreading Seas, with `actor`'s
/// Urborg, Tomb of Yawgmoth entering after the Seas when `urborg_later`, or
/// on the battlefield before it.
fn volcanic_under_spreading_seas_with_urborg(
    actor: PlayerId,
    urborg_later: bool,
) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let volcanic = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
    let urborg = put(&mut state, actor, "Urborg, Tomb of Yawgmoth", Zone::Hand);
    if !urborg_later {
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(urborg, Zone::Battlefield),
        );
        settled(&mut state);
    }
    cast_from_pool(
        &mut state,
        actor,
        "Spreading Seas",
        &[ManaColor::U],
        1,
        &[Target::Object(volcanic)],
    );
    if urborg_later {
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(urborg, Zone::Battlefield),
        );
        settled(&mut state);
    }
    assert_eq!(state.objects.get(urborg).zone, Zone::Battlefield);
    (state, volcanic)
}

const WUBRG: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

/// Abundant Growth's grant survives a land-type setter (305.7): a Volcanic
/// Island with it taps for any color under Blood Moon or Spreading Seas, by
/// hand and in the planner, and pays for Brainstorm under Blood Moon.
#[test]
fn abundant_growth_keeps_any_color_under_a_land_type_setter_both_seats() {
    for actor in [P0, P1] {
        for setter in ["Blood Moon", "Spreading Seas"] {
            let (mut state, volcanic) = volcanic_with_growth_under(actor, setter);
            assert_offers(&mut state, volcanic, &WUBRG, setter);
        }
        let (mut state, volcanic) = volcanic_with_growth_under(actor, "Blood Moon");
        let brainstorm = put(&mut state, actor, "Brainstorm", Zone::Hand);
        cast(&mut state, brainstorm, &[]);
        settled_choosing_first(&mut state);
        assert!(state.objects.get(volcanic).tapped);
        assert_eq!(state.objects.get(brainstorm).zone, Zone::Graveyard);
    }
}

/// Petrified Hamlet's grant to the lands it names, "{T}: Add {C}.", survives
/// Spreading Seas: an Urza's Tower under the Seas taps for {U} or {C}, and
/// pays the {C} of Thought-Knot Seer.
#[test]
fn petrified_hamlets_grant_survives_spreading_seas_both_seats() {
    for actor in [P0, P1] {
        let (mut state, tower) = tower_named_by_hamlet_under_spreading_seas(actor);
        assert_offers(&mut state, tower, &[ManaColor::U, ManaColor::C], "Tower");
        let seer = put(&mut state, actor, "Thought-Knot Seer", Zone::Hand);
        pool(
            &mut state,
            actor,
            &[ManaColor::U, ManaColor::U, ManaColor::U],
            0,
        );
        cast(&mut state, seer, &[]);
        settled_choosing_first(&mut state);
        assert!(state.objects.get(tower).tapped);
        assert_eq!(state.objects.get(seer).zone, Zone::Battlefield);
    }
}

/// Navigator's Compass and Spreading Seas both change land types, in
/// timestamp order (613.7): a Mountain the Compass adds after the Seas stays
/// and taps for {R}; one it added before the Seas is gone.
#[test]
fn navigators_compass_and_spreading_seas_apply_in_timestamp_order_both_seats() {
    for actor in [P0, P1] {
        let opponent = opponent_of(actor);
        let (mut state, volcanic) = volcanic_under_spreading_seas_then_compass(actor);
        assert!(engine::has_effective_subtype(
            &state,
            volcanic,
            mtg_kernel::card_def::Subtype::Mountain
        ));
        assert_offers(
            &mut state,
            volcanic,
            &[ManaColor::U, ManaColor::R],
            "Compass after the Seas",
        );
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        cast(&mut state, bolt, &[Target::Player(opponent)]);
        settled(&mut state);
        assert!(state.objects.get(volcanic).tapped);
        assert_eq!(state.players[opponent.index()].life, 17);

        let (mut state, volcanic) = volcanic_under_compass_then_spreading_seas(actor);
        assert!(!engine::has_effective_subtype(
            &state,
            volcanic,
            mtg_kernel::card_def::Subtype::Mountain
        ));
        assert_offers(
            &mut state,
            volcanic,
            &[ManaColor::U],
            "Compass before the Seas",
        );
    }
}

/// Urborg, Tomb of Yawgmoth and Spreading Seas, in timestamp order (613.7):
/// an Urborg that entered after the Seas makes the Island a Swamp too, one
/// that was there before does not.
#[test]
fn urborg_and_spreading_seas_apply_in_timestamp_order_both_seats() {
    for actor in [P0, P1] {
        let (mut state, volcanic) = volcanic_under_spreading_seas_with_urborg(actor, true);
        assert_offers(
            &mut state,
            volcanic,
            &[ManaColor::U, ManaColor::B],
            "Urborg after the Seas",
        );
        let (mut state, volcanic) = volcanic_under_spreading_seas_with_urborg(actor, false);
        assert_offers(
            &mut state,
            volcanic,
            &[ManaColor::U],
            "Urborg before the Seas",
        );
    }
}

/// Unchanged: a land whose printed abilities stay keeps its printed colors
/// beside the land types Urborg and Yavimaya add, and a land with no effect
/// on it is planned for its printed colors.
#[test]
fn land_grants_beside_active_printed_abilities_are_planned_as_before_both_seats() {
    for actor in [P0, P1] {
        let (state, volcanic) = volcanic_with_urborg_and_yavimaya(actor);
        let source = planned(&state, actor, volcanic).expect("a source");
        assert_eq!(source.yield_per_tap, 1);
        assert_eq!(
            sorted(&source.choices),
            vec![ManaColor::U, ManaColor::B, ManaColor::R, ManaColor::G]
        );

        let (mut state, volcanic) = plain_volcanic(actor);
        let source = planned(&state, actor, volcanic).expect("a source");
        assert_eq!(source.yield_per_tap, 1);
        assert_eq!(sorted(&source.choices), vec![ManaColor::U, ManaColor::R]);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let brainstorm = put(&mut state, actor, "Brainstorm", Zone::Hand);
        assert!(castable(&mut state, bolt));
        assert!(castable(&mut state, brainstorm));
    }
}

// ---- A removal of all abilities and the mana of a land type -------------------------

/// `actor`'s Dryad Arbor under `actor`'s Dress Down ("Creatures lose all
/// abilities"), then `after`: Yavimaya, Cradle of Growth entering, Navigator's
/// Compass making the Arbor a Forest, or Abundant Growth enchanting it.
fn dryad_arbor_under_dress_down_then(actor: PlayerId, after: &str) -> (GameState, ObjectId) {
    let mut state = game(actor);
    let arbor = put(&mut state, actor, "Dryad Arbor", Zone::Battlefield);
    cast_from_pool(&mut state, actor, "Dress Down", &[ManaColor::U], 1, &[]);
    match after {
        "Yavimaya, Cradle of Growth" => {
            let yavimaya = put(&mut state, actor, after, Zone::Hand);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(yavimaya, Zone::Battlefield),
            );
            settled(&mut state);
        }
        "Navigator's Compass" => compass_adds(&mut state, actor, arbor, 4),
        "Abundant Growth" => {
            let growth = put(&mut state, actor, after, Zone::Hand);
            pool(&mut state, actor, &[ManaColor::G], 0);
            cast(&mut state, growth, &[Target::Object(arbor)]);
            settled(&mut state);
            assert_eq!(state.objects.get(growth).zone, Zone::Battlefield);
        }
        other => panic!("{other}"),
    }
    assert!(!state.objects.get(arbor).tapped);
    (state, arbor)
}

/// The mana ability that comes with a basic land type belongs to the type
/// (305.6), and Dress Down's "lose all abilities" removes it in layer 6
/// whatever the timestamp of the effect that gave the type (613.1): a Dryad
/// Arbor under Dress Down does not tap for {G} when Yavimaya enters or
/// Navigator's Compass makes it a Forest later, by hand or in the planner.
#[test]
fn a_later_land_type_does_not_give_mana_through_dress_down_both_seats() {
    for actor in [P0, P1] {
        for after in ["Yavimaya, Cradle of Growth", "Navigator's Compass"] {
            let (mut state, arbor) = dryad_arbor_under_dress_down_then(actor, after);
            assert!(engine::has_effective_subtype(
                &state,
                arbor,
                mtg_kernel::card_def::Subtype::Forest
            ));
            next(&mut state);
            assert_eq!(tapped_colors(&state, arbor), Vec::new(), "{after}: by hand");
            assert!(planned(&state, actor, arbor).is_none(), "{after}: planner");
        }
    }
}

/// Abundant Growth's grant is a layer-6 effect: one that enchants a Dryad
/// Arbor after Dress Down has the later timestamp and applies (613.7), so the
/// Arbor taps for any color.
#[test]
fn abundant_growth_after_dress_down_gives_any_color_both_seats() {
    for actor in [P0, P1] {
        let (mut state, arbor) = dryad_arbor_under_dress_down_then(actor, "Abundant Growth");
        assert_offers(
            &mut state,
            arbor,
            &WUBRG,
            "Abundant Growth after Dress Down",
        );
    }
}

/// A Navigator's Compass record saved before records carried a timestamp
/// loads with timestamp 0, so a land type it added under a land-type setter
/// counts as earlier than the setter: the land has the setter's type only.
#[test]
fn a_compass_record_without_a_timestamp_loads_as_earlier_than_the_setter_both_seats() {
    for actor in [P0, P1] {
        let (state, volcanic) = volcanic_under_spreading_seas_then_compass(actor);
        let mut json = serde_json::to_value(&state).unwrap();
        let records = json["eternal_v1"]["s1d"]["extra_land_types"]
            .as_array_mut()
            .expect("the Compass record");
        assert_eq!(records.len(), 1);
        assert!(records[0]["timestamp"].as_u64().unwrap() > 0);
        records[0].as_object_mut().unwrap().remove("timestamp");
        let mut loaded: GameState = serde_json::from_value(json).unwrap();
        let reloaded = serde_json::to_value(&loaded).unwrap();
        assert_eq!(
            reloaded["eternal_v1"]["s1d"]["extra_land_types"][0]["timestamp"],
            serde_json::json!(0)
        );
        assert!(!engine::has_effective_subtype(
            &loaded,
            volcanic,
            mtg_kernel::card_def::Subtype::Mountain
        ));
        assert_offers(&mut loaded, volcanic, &[ManaColor::U], "an old record");
    }
}
