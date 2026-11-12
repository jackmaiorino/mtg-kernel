//! Legacy sideboard wave S1m: native rules tests for Extinguisher
//! Battleship (its entry trigger, station and station 5+) and Fiery
//! Confluence, and the deck games that play both cards through public
//! sessions.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, Policy, FIERY_CONFLUENCE_DESTROY_MODES};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, CostComponent, Keywords, Subtype,
    TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{
    GameObject, GameState, ObjectLinkV4, ObjectStateV4, StackItemKind, Step, Target, Zone,
};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const BATTLESHIP: &str = "Extinguisher Battleship";
const CONFLUENCE: &str = "Fiery Confluence";
/// A 5/5 creature with no ability that matters here.
const ANGLER: &str = "Gurmag Angler";
/// A 1/2 creature.
const INSPECTOR: &str = "Thraben Inspector";
/// A 0/1 creature.
const BIRDS: &str = "Birds of Paradise";

fn def(name: &str) -> &'static CardDef {
    &CARD_DEFS[id(name) as usize]
}

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A first main phase with `active` active and holding priority, both
/// libraries twenty Islands.
fn game(active: PlayerId) -> GameState {
    let ids = vec![id("Island"); 20];
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x5331_4d00_0001,
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

fn charge(state: &GameState, object: ObjectId) -> i32 {
    state.objects.get(object).counters.charge
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

/// The spells and abilities the priority player may cast or activate.
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

/// Whether the priority player may activate the Battleship's station.
fn can_station(state: &mut GameState, battleship: ObjectId) -> bool {
    priority(state).1.contains(&(battleship, 0))
}

/// Casts `spell` with the mana already in its caster's pool.
fn cast(state: &mut GameState, spell: ObjectId) {
    assert!(
        priority(state).0.contains(&spell),
        "{} not castable",
        state.objects.get(spell).name
    );
    act(state, Action::CastSpell(spell));
}

/// Casts the Battleship from its caster's hand and lets it resolve, up to
/// its entry trigger's target choice. Returns the legal targets.
fn cast_battleship(state: &mut GameState, battleship: ObjectId, caster: PlayerId) -> Vec<Target> {
    pool(state, caster, &[], 8);
    cast(state, battleship);
    match settle(state) {
        Some(Decision::ChooseTargets { legal_targets, .. }) => {
            assert_eq!(zone(state, battleship), Zone::Battlefield);
            legal_targets
        }
        other => panic!("expected the entry trigger's target, got {other:?}"),
    }
}

/// Answers the entry trigger's target prompt with `target` and lets the
/// engine put the trigger on the stack, so that its controller holds
/// priority with it there.
fn choose_target(state: &mut GameState, target: Target) {
    act(state, Action::ChooseTarget(target));
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}

/// Activates the Battleship's station tapping `creature`, up to its
/// activator holding priority with the ability on the stack. The engine
/// asks for the creature only when there is more than one candidate.
fn station(state: &mut GameState, battleship: ObjectId, creature: ObjectId) {
    assert!(can_station(state, battleship), "station not offered");
    act(state, Action::ActivateAbility(battleship, 0));
    if let Decision::ChooseCostTargets { candidates, .. } = next(state) {
        assert!(candidates.contains(&creature), "{candidates:?}");
        assert!(!candidates.contains(&battleship), "another creature");
        act(state, Action::ChooseCostTarget(creature));
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    }
    assert!(state.objects.get(creature).tapped, "the cost taps it");
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == battleship && item.kind == StackItemKind::ActivatedAbility));
}

/// Moves `object` to its owner's graveyard, as if destroyed.
fn destroy(state: &mut GameState, object: ObjectId) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, Zone::Graveyard));
}

/// `thief` gains control of `object`, a creature its opponent owns.
fn steal(state: &mut GameState, object: ObjectId, thief: PlayerId) {
    state.players[thief.opponent().index()]
        .battlefield
        .retain(|&id| id != object);
    state.players[thief.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = thief;
}

fn power_toughness(state: &GameState, object: ObjectId) -> (i32, i32) {
    (
        engine::effective_power(state, object),
        engine::effective_toughness(state, object),
    )
}

/// Both keyword queries agree on each of `keywords`; returns which the
/// object has.
fn keywords(state: &GameState, object: ObjectId, keywords: Keywords) -> Keywords {
    let found = engine::effective_keywords(state, object, keywords);
    for keyword in [Keywords::FLYING, Keywords::TRAMPLE] {
        if keywords.has(keyword) {
            assert_eq!(
                found.has(keyword),
                engine::has_effective_keyword(state, object, keyword)
            );
        }
    }
    found
}

fn flying_trample(state: &GameState, object: ObjectId) -> (bool, bool) {
    let found = keywords(state, object, Keywords::FLYING | Keywords::TRAMPLE);
    (found.has(Keywords::FLYING), found.has(Keywords::TRAMPLE))
}

fn is_creature(state: &GameState, object: ObjectId) -> bool {
    engine::object_has_type(state, object, CardType::Creature)
}

/// The damage events dealt by `source` after the first `from` events of
/// the history, in order.
fn damage_events(state: &GameState, source: ObjectId, from: usize) -> Vec<(Target, i32)> {
    state
        .engine
        .event_history
        .iter()
        .skip(from)
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: dealt_by,
                target,
                amount,
            } if *dealt_by == source => Some((*target, *amount)),
            _ => None,
        })
        .collect()
}

// ---- Characteristics -----------------------------------------------------------

#[test]
fn battleship_and_confluence_are_full_with_their_printed_characteristics() {
    let battleship = def(BATTLESHIP);
    assert_eq!(battleship.capability, CardCapability::Full);
    assert!(battleship.is_executable() && !battleship.is_token);
    assert_eq!(
        (
            battleship.cost.generic,
            battleship.cost.pips.len(),
            battleship.cost.x_count
        ),
        (8, 0, 0)
    );
    assert_eq!(battleship.mana_value, 8);
    assert_eq!(battleship.types, &[CardType::Artifact]);
    assert_eq!(battleship.subtypes, &[Subtype::Spacecraft]);
    assert!(!Subtype::Spacecraft.is_creature_type());
    assert_eq!((battleship.power, battleship.toughness), (None, None));
    assert!(!battleship.keywords.has(Keywords::FLYING));
    assert!(!battleship.keywords.has(Keywords::TRAMPLE));
    let [station] = battleship.activated_abilities else {
        panic!("one activated ability");
    };
    assert_eq!(
        station.cost,
        &[CostComponent::TapOtherUntappedControlledCreatureV1]
    );
    assert!(station.sorcery_speed_only);
    assert_eq!(station.activation_zone, Zone::Battlefield);
    assert_eq!(station.target_spec, TargetSpec::None);

    let confluence = def(CONFLUENCE);
    assert_eq!(confluence.capability, CardCapability::Full);
    assert_eq!(
        (
            confluence.cost.generic,
            confluence.cost.pips.len(),
            confluence.cost.x_count
        ),
        (2, 2, 0)
    );
    assert_eq!(confluence.mana_value, 4);
    assert_eq!(confluence.types, &[CardType::Sorcery]);
    assert!(confluence.mode2.is_some() && confluence.mode3.is_some());
    assert_eq!(confluence.additional_modes.len(), 7);
}

/// Both cards are offered once their caster's untapped lands can pay for
/// them, as automatic payment does in sessions.
#[test]
fn battleship_and_confluence_are_castable_from_untapped_lands_both_seats() {
    for caster in [P0, P1] {
        let mut state = game(caster);
        let battleship = put(&mut state, caster, BATTLESHIP, Zone::Hand);
        let confluence = put(&mut state, caster, CONFLUENCE, Zone::Hand);
        put(&mut state, caster, "Mountain", Zone::Battlefield);
        for _ in 0..2 {
            put(&mut state, caster, "Island", Zone::Battlefield);
        }
        let castable = priority(&mut state).0;
        assert!(!castable.contains(&confluence) && !castable.contains(&battleship));
        put(&mut state, caster, "Island", Zone::Battlefield);
        let castable = priority(&mut state).0;
        assert!(
            !castable.contains(&confluence),
            "four lands but one red source"
        );
        put(&mut state, caster, "Mountain", Zone::Battlefield);
        let castable = priority(&mut state).0;
        assert!(castable.contains(&confluence) && !castable.contains(&battleship));
        for _ in 0..2 {
            put(&mut state, caster, "Island", Zone::Battlefield);
        }
        assert!(!priority(&mut state).0.contains(&battleship), "seven lands");
        put(&mut state, caster, "Island", Zone::Battlefield);
        assert!(priority(&mut state).0.contains(&battleship), "eight lands");
    }
}

// ---- Entry trigger -----------------------------------------------------------------

/// The Battleship's entry trigger targets a noncreature permanent (itself
/// included), destroys it, then deals 4 damage to each creature.
#[test]
fn battleship_entering_destroys_a_noncreature_permanent_then_damages_each_creature_both_seats() {
    for caster in [P0, P1] {
        for victim_name in ["Island", "Sol Ring"] {
            let mut state = game(caster);
            let battleship = put(&mut state, caster, BATTLESHIP, Zone::Hand);
            let victim = put(
                &mut state,
                caster.opponent(),
                victim_name,
                Zone::Battlefield,
            );
            let own_small = put(&mut state, caster, INSPECTOR, Zone::Battlefield);
            let their_large = put(&mut state, caster.opponent(), ANGLER, Zone::Battlefield);
            let legal = cast_battleship(&mut state, battleship, caster);
            assert!(legal.contains(&Target::Object(victim)));
            assert!(legal.contains(&Target::Object(battleship)), "itself");
            assert!(!legal.contains(&Target::Object(own_small)), "a creature");
            assert!(!legal.contains(&Target::Object(their_large)), "a creature");
            choose_target(&mut state, Target::Object(victim));
            settled(&mut state);
            assert_eq!(zone(&state, victim), Zone::Graveyard, "{victim_name}");
            assert_eq!(zone(&state, own_small), Zone::Graveyard, "4 damage");
            assert_eq!(zone(&state, their_large), Zone::Battlefield);
            assert_eq!(state.objects.get(their_large).damage, 4);
            assert_eq!(zone(&state, battleship), Zone::Battlefield);
        }
    }
}

/// If the target leaves before the trigger resolves, the trigger doesn't
/// resolve and deals no damage; targeting itself, the Battleship is
/// destroyed and still deals the damage.
#[test]
fn battleship_trigger_deals_no_damage_without_its_target_and_may_target_itself_both_seats() {
    for caster in [P0, P1] {
        let mut state = game(caster);
        let battleship = put(&mut state, caster, BATTLESHIP, Zone::Hand);
        let victim = put(&mut state, caster.opponent(), "Island", Zone::Battlefield);
        let creature = put(&mut state, caster.opponent(), ANGLER, Zone::Battlefield);
        cast_battleship(&mut state, battleship, caster);
        choose_target(&mut state, Target::Object(victim));
        destroy(&mut state, victim);
        settled(&mut state);
        assert_eq!(state.objects.get(creature).damage, 0, "no damage");

        let mut state = game(caster);
        let battleship = put(&mut state, caster, BATTLESHIP, Zone::Hand);
        let creature = put(&mut state, caster.opponent(), ANGLER, Zone::Battlefield);
        cast_battleship(&mut state, battleship, caster);
        choose_target(&mut state, Target::Object(battleship));
        settled(&mut state);
        assert_eq!(zone(&state, battleship), Zone::Graveyard);
        assert_eq!(state.objects.get(creature).damage, 4);
    }
}

// ---- Station ------------------------------------------------------------------------

/// Station puts charge counters equal to the tapped creature's power as the
/// ability resolves; a summoning-sick creature, or one its opponent owns,
/// can be tapped.
#[test]
fn station_counts_the_tapped_creatures_power_on_resolution_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let angler = put(&mut state, player, ANGLER, Zone::Battlefield);
        let inspector = put(&mut state, player, INSPECTOR, Zone::Battlefield);
        state.objects.get_mut(angler).summoning_sick = true;
        station(&mut state, battleship, angler);
        settled(&mut state);
        assert_eq!(charge(&state, battleship), 5);

        // The power as the ability resolves: a counter added in response.
        station(&mut state, battleship, inspector);
        state.objects.get_mut(inspector).counters.plus1_plus1 += 1;
        settled(&mut state);
        assert_eq!(charge(&state, battleship), 7);

        // A creature its opponent owns.
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let stolen = put(&mut state, player.opponent(), ANGLER, Zone::Battlefield);
        steal(&mut state, stolen, player);
        station(&mut state, battleship, stolen);
        settled(&mut state);
        assert_eq!(charge(&state, battleship), 5);
    }
}

/// The tapped creature's power as it last existed on the battlefield counts
/// when it left; power 0 gives no counters; nothing happens when the
/// Battleship left.
#[test]
fn station_uses_last_known_power_and_needs_positive_power_and_the_battleship_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let angler = put(&mut state, player, ANGLER, Zone::Battlefield);
        station(&mut state, battleship, angler);
        state.objects.get_mut(angler).counters.plus1_plus1 += 1;
        destroy(&mut state, angler);
        settled(&mut state);
        assert_eq!(charge(&state, battleship), 6, "last-known power");

        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let birds = put(&mut state, player, BIRDS, Zone::Battlefield);
        station(&mut state, battleship, birds);
        settled(&mut state);
        assert_eq!(charge(&state, battleship), 0, "power 0");

        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let angler = put(&mut state, player, ANGLER, Zone::Battlefield);
        station(&mut state, battleship, angler);
        destroy(&mut state, battleship);
        settled(&mut state);
        assert_eq!(zone(&state, battleship), Zone::Graveyard);
        assert_eq!(charge(&state, battleship), 0);
        assert!(state.objects.get(angler).tapped);
    }
}

/// Station needs another untapped creature and sorcery timing; a stationed
/// Battleship can't tap itself.
#[test]
fn station_is_offered_only_at_sorcery_speed_with_another_untapped_creature_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        assert!(!can_station(&mut state, battleship), "no creature");
        let angler = put(&mut state, player, ANGLER, Zone::Battlefield);
        state.objects.get_mut(angler).tapped = true;
        assert!(!can_station(&mut state, battleship), "a tapped creature");
        state.objects.get_mut(battleship).counters.charge = 5;
        assert!(is_creature(&state, battleship));
        assert!(!can_station(&mut state, battleship), "not itself");
        state.objects.get_mut(angler).tapped = false;
        assert!(
            can_station(&mut state, battleship),
            "at any number of counters"
        );

        // Not while something is on the stack, nor on the opponent's turn.
        let inspector = put(&mut state, player, INSPECTOR, Zone::Battlefield);
        station(&mut state, battleship, angler);
        assert!(
            !can_station(&mut state, battleship),
            "the stack isn't empty"
        );
        settled(&mut state);
        state.active_player = player.opponent();
        state.priority_player = player;
        assert!(!can_station(&mut state, battleship), "the opponent's turn");
        assert!(!state.objects.get(inspector).tapped);
    }
}

// ---- Station 5+ ---------------------------------------------------------------------

/// With five or more charge counters, the Battleship is a 10/10 artifact
/// creature with flying and trample; with four, or off the battlefield, it
/// isn't a creature and has no power or toughness. It attacks once it has
/// been controlled since the turn began.
#[test]
fn stationed_battleship_is_a_ten_ten_flying_trample_creature_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        state.objects.get_mut(battleship).counters.charge = 4;
        assert!(!is_creature(&state, battleship));
        assert_eq!(engine::effective_base_power(&state, battleship), None);
        assert_eq!(flying_trample(&state, battleship), (false, false));
        state.objects.get_mut(battleship).counters.charge = 5;
        assert!(is_creature(&state, battleship));
        assert!(engine::object_has_type(
            &state,
            battleship,
            CardType::Artifact
        ));
        assert!(engine::has_effective_subtype(
            &state,
            battleship,
            Subtype::Spacecraft
        ));
        assert_eq!(power_toughness(&state, battleship), (10, 10));
        assert_eq!(engine::effective_base_power(&state, battleship), Some(10));
        assert_eq!(flying_trample(&state, battleship), (true, true));

        let in_hand = put(&mut state, player, BATTLESHIP, Zone::Hand);
        state.objects.get_mut(in_hand).counters.charge = 5;
        assert!(!is_creature(&state, in_hand));
        assert_eq!(engine::effective_base_power(&state, in_hand), None);

        let fresh = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        state.objects.get_mut(fresh).counters.charge = 5;
        state.objects.get_mut(fresh).summoning_sick = true;
        let eligible = attackers(&mut state);
        assert!(eligible.contains(&battleship));
        assert!(!eligible.contains(&fresh), "entered this turn");
    }
}

/// Passes priority to the active player's declare attackers step and
/// returns the creatures that may attack.
fn attackers(state: &mut GameState) -> Vec<ObjectId> {
    for _ in 0..20 {
        match next(state) {
            Decision::DeclareAttackers { eligible, .. } => return eligible,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("expected attackers, got {other:?}"),
        }
    }
    panic!("no declare attackers step");
}

/// Karn, the Great Creator's +1 and the Tezzeret, Cruel Captain emblem's
/// Robot set the Battleship's base power and toughness later than its
/// station symbol, so theirs apply; it still gets flying and trample.
#[test]
fn karn_and_the_tezzeret_robot_keep_their_base_power_and_toughness_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        let karn = put(&mut state, player, "Karn, the Great Creator", Zone::Hand);
        pool(&mut state, player, &[], 4);
        cast(&mut state, karn);
        settled(&mut state);
        assert!(priority(&mut state).1.contains(&(karn, 0)));
        act(&mut state, Action::ActivateAbility(karn, 0));
        // "Up to one target noncreature artifact" is chosen through the
        // effect-target prompt.
        match next(&mut state) {
            Decision::ChooseEffectTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&Target::Object(battleship)));
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(battleship)),
                );
                if let Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } = next(&mut state)
                {
                    act(&mut state, Action::FinishEffectSelection);
                }
            }
            other => panic!("expected Karn's target, got {other:?}"),
        }
        settled(&mut state);
        assert_eq!(power_toughness(&state, battleship), (8, 8));
        state.objects.get_mut(battleship).counters.charge = 5;
        assert_eq!(power_toughness(&state, battleship), (8, 8), "Karn's");
        assert_eq!(flying_trample(&state, battleship), (true, true));

        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        // The emblem's resolution: three +1/+1 counters on a noncreature
        // artifact, which becomes a 0/0 Robot artifact creature.
        state.objects.get_mut(battleship).counters.plus1_plus1 = 3;
        let link = ObjectLinkV4 {
            object: battleship,
            zone_change_count: state.objects.get(battleship).zone_change_count,
        };
        state
            .eternal_v1
            .get_or_insert_with(Default::default)
            .vintage
            .robots
            .push(link);
        assert_eq!(power_toughness(&state, battleship), (3, 3));
        state.objects.get_mut(battleship).counters.charge = 5;
        assert_eq!(power_toughness(&state, battleship), (3, 3), "the Robot's");
        assert_eq!(flying_trample(&state, battleship), (true, true));
    }
}

/// Dress Down removes a stationed Battleship's printed abilities, station
/// among them, and it stays a 10/10 creature. Its flying and trample go
/// only when Dress Down is later than the Battleship. With four counters
/// the Battleship isn't a creature, so it keeps station.
#[test]
fn dress_down_removes_station_keywords_only_when_later_both_seats() {
    for player in [P0, P1] {
        let mut state = game(player);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Battlefield);
        put(&mut state, player, ANGLER, Zone::Battlefield);
        state.objects.get_mut(battleship).counters.charge = 5;
        let dress = put(&mut state, player, "Dress Down", Zone::Hand);
        pool(&mut state, player, &[ManaColor::U], 1);
        cast(&mut state, dress);
        settled(&mut state);
        assert_eq!(zone(&state, dress), Zone::Battlefield);
        assert_eq!(flying_trample(&state, battleship), (false, false));
        assert!(is_creature(&state, battleship));
        assert_eq!(power_toughness(&state, battleship), (10, 10));
        assert!(!can_station(&mut state, battleship), "lost its abilities");

        // Dress Down first: the Battleship enters later, so its station
        // symbol's flying and trample apply after Dress Down's removal.
        let mut state = game(player);
        let dress = put(&mut state, player, "Dress Down", Zone::Hand);
        pool(&mut state, player, &[ManaColor::U], 1);
        cast(&mut state, dress);
        settled(&mut state);
        let land = put(&mut state, player.opponent(), "Island", Zone::Battlefield);
        let battleship = put(&mut state, player, BATTLESHIP, Zone::Hand);
        cast_battleship(&mut state, battleship, player);
        choose_target(&mut state, Target::Object(land));
        settled(&mut state);
        state.objects.get_mut(battleship).counters.charge = 5;
        assert_eq!(flying_trample(&state, battleship), (true, true), "earlier");
        assert_eq!(power_toughness(&state, battleship), (10, 10));
        // As a creature it still loses its printed abilities; at four
        // counters it isn't one.
        put(&mut state, player, ANGLER, Zone::Battlefield);
        assert!(!can_station(&mut state, battleship), "a creature");
        state.objects.get_mut(battleship).counters.charge = 4;
        assert!(can_station(&mut state, battleship), "not a creature");
    }
}

// ---- Fiery Confluence ---------------------------------------------------------------

/// Casts Fiery Confluence for its `mode` choice, naming `targets` in order.
fn cast_confluence(
    state: &mut GameState,
    caster: PlayerId,
    mode: u8,
    targets: &[Target],
) -> ObjectId {
    let confluence = put(state, caster, CONFLUENCE, Zone::Hand);
    pool(state, caster, &[ManaColor::R, ManaColor::R], 2);
    cast(state, confluence);
    match next(state) {
        Decision::ChooseSpellMode { legal_modes, .. } => {
            assert!(legal_modes.contains(&mode), "{legal_modes:?}");
            act(state, Action::ChooseSpellMode(mode));
        }
        other => panic!("expected the mode choice, got {other:?}"),
    }
    for &target in targets {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                assert!(legal_targets.contains(&target), "{target:?}");
                act(state, Action::ChooseTarget(target));
            }
            other => panic!("expected targets, got {other:?}"),
        }
    }
    // The cast completes as the engine next advances: its caster then holds
    // priority with the spell on the stack.
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == confluence));
    confluence
}

/// The modes with a destroy are offered only while an artifact is on the
/// battlefield.
#[test]
fn fiery_confluence_offers_destroy_modes_only_with_an_artifact_both_seats() {
    for caster in [P0, P1] {
        for with_artifact in [false, true] {
            let mut state = game(caster);
            if with_artifact {
                put(&mut state, caster.opponent(), "Sol Ring", Zone::Battlefield);
            }
            let confluence = put(&mut state, caster, CONFLUENCE, Zone::Hand);
            pool(&mut state, caster, &[ManaColor::R, ManaColor::R], 2);
            cast(&mut state, confluence);
            match next(&mut state) {
                Decision::ChooseSpellMode {
                    mode_count,
                    legal_modes,
                    ..
                } => {
                    assert_eq!(mode_count, 10);
                    let expected: Vec<u8> = if with_artifact {
                        (0..10).collect()
                    } else {
                        vec![0, 1, 3, 6]
                    };
                    assert_eq!(legal_modes, expected);
                }
                other => panic!("expected the mode choice, got {other:?}"),
            }
        }
    }
}

/// Each damage mode is its own damage event, in printed order (creatures,
/// then the opponent, then the destroy), and two destroy modes may name the
/// same artifact.
#[test]
fn fiery_confluence_resolves_its_modes_in_printed_order_as_separate_events_both_seats() {
    for caster in [P0, P1] {
        let opponent = caster.opponent();
        // [1 damage, 1 damage, 1 damage]: three events to each creature.
        let mut state = game(caster);
        let angler = put(&mut state, opponent, ANGLER, Zone::Battlefield);
        let inspector = put(&mut state, caster, INSPECTOR, Zone::Battlefield);
        let from = state.engine.event_history.len();
        let confluence = cast_confluence(&mut state, caster, 0, &[]);
        settled(&mut state);
        let events = damage_events(&state, confluence, from);
        assert_eq!(
            events
                .iter()
                .filter(|&&event| event == (Target::Object(angler), 1))
                .count(),
            3
        );
        assert_eq!(state.objects.get(angler).damage, 3);
        assert_eq!(zone(&state, inspector), Zone::Graveyard);

        // [creatures, opponent, destroy]: in printed order.
        let mut state = game(caster);
        let angler = put(&mut state, opponent, ANGLER, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let from = state.engine.event_history.len();
        let confluence = cast_confluence(&mut state, caster, 4, &[Target::Object(ring)]);
        settled(&mut state);
        assert_eq!(
            damage_events(&state, confluence, from),
            vec![(Target::Object(angler), 1), (Target::Player(opponent), 2)]
        );
        assert_eq!(zone(&state, ring), Zone::Graveyard);
        assert_eq!(state.players[opponent.index()].life, 18);

        // [opponent, opponent, opponent]: three events of 2.
        let mut state = game(caster);
        let from = state.engine.event_history.len();
        let confluence = cast_confluence(&mut state, caster, 6, &[]);
        settled(&mut state);
        assert_eq!(
            damage_events(&state, confluence, from),
            vec![(Target::Player(opponent), 2); 3]
        );
        assert_eq!(state.players[opponent.index()].life, 14);

        // [destroy, destroy, destroy] naming Sol Ring twice and Mana Crypt
        // once.
        let mut state = game(caster);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let crypt = put(&mut state, opponent, "Mana Crypt", Zone::Battlefield);
        cast_confluence(
            &mut state,
            caster,
            9,
            &[
                Target::Object(ring),
                Target::Object(ring),
                Target::Object(crypt),
            ],
        );
        settled(&mut state);
        assert_eq!(zone(&state, ring), Zone::Graveyard);
        assert_eq!(zone(&state, crypt), Zone::Graveyard);
    }
}

/// With every artifact target illegal, Fiery Confluence doesn't resolve and
/// deals no damage; with one of two left, the rest happens.
#[test]
fn fiery_confluence_does_not_resolve_when_every_target_is_illegal_both_seats() {
    for caster in [P0, P1] {
        let opponent = caster.opponent();
        // [1 damage, 1 damage, destroy] whose artifact left.
        let mut state = game(caster);
        let angler = put(&mut state, opponent, ANGLER, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let confluence = cast_confluence(&mut state, caster, 2, &[Target::Object(ring)]);
        destroy(&mut state, ring);
        settled(&mut state);
        assert_eq!(zone(&state, confluence), Zone::Graveyard);
        assert_eq!(state.objects.get(angler).damage, 0, "didn't resolve");

        // [1 damage, destroy, destroy] with one of two artifacts gone.
        let mut state = game(caster);
        let angler = put(&mut state, opponent, ANGLER, Zone::Battlefield);
        let ring = put(&mut state, opponent, "Sol Ring", Zone::Battlefield);
        let crypt = put(&mut state, opponent, "Mana Crypt", Zone::Battlefield);
        cast_confluence(
            &mut state,
            caster,
            5,
            &[Target::Object(ring), Target::Object(crypt)],
        );
        destroy(&mut state, ring);
        settled(&mut state);
        assert_eq!(zone(&state, crypt), Zone::Graveyard);
        assert_eq!(state.objects.get(angler).damage, 1);
    }
}

// ---- Deck games -----------------------------------------------------------------

fn named_card(card: &Value, name: &str) -> bool {
    card["card_db_id"]
        .as_u64()
        .is_some_and(|id| CARD_DEFS[id as usize].name == name)
}

/// Counts, over one session game, the Battleship entry triggers that left
/// the public stack, station activations chosen from the action list,
/// decisions that show a stationed Battleship as a creature, and Fiery
/// Confluence mode choices with a destroy mode.
#[derive(Default)]
struct DeckEvents {
    entry_triggers: usize,
    stations: usize,
    stationed_creatures: usize,
    destroy_modes: usize,
    /// Battleship triggers on the stack at the last decision.
    triggers_on_stack: usize,
}

impl DeckEvents {
    fn observe(&mut self, decision: &Value, action: &Value) {
        let projection = &decision["observation"]["projection"];
        let triggers = projection["stack"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| {
                item["stack_item_kind"] == "triggered_ability"
                    && named_card(&item["source"], BATTLESHIP)
            })
            .count();
        self.entry_triggers += self.triggers_on_stack.saturating_sub(triggers);
        self.triggers_on_stack = triggers;
        self.stationed_creatures += usize::from(
            projection["battlefield"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|cards| cards.as_array().into_iter().flatten())
                .any(|card| {
                    named_card(&card["stable"], BATTLESHIP)
                        && card["counters"]["charge"].as_i64().unwrap_or(0) >= 5
                        && card["characteristics"]["type_flags"]["creature"] == true
                        && card["characteristics"]["effective_power"] == 10
                }),
        );
        let semantic = &action["semantic"];
        if semantic["action_kind"] == "activate_ability"
            && named_card(&semantic["source"], BATTLESHIP)
        {
            self.stations += 1;
        }
        if semantic["action_kind"] == "choose_spell_mode"
            && named_card(&semantic["source"], CONFLUENCE)
            && semantic["mode_index"]
                .as_u64()
                .is_some_and(|mode| FIERY_CONFLUENCE_DESTROY_MODES.contains(&mode))
        {
            self.destroy_modes += 1;
        }
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

/// The Battleship and Fiery Confluence are sideboard cards in their lists,
/// which the best-of-three harness rarely draws, so a colorless deck with
/// twelve Battleships, fast mana and creatures to station with and a red
/// deck of Mountains and twelve Fiery Confluences play each other through
/// public sessions, both seats with ranked and with seeded random actions.
/// Ranked seats station a Battleship until it is a creature and choose
/// Fiery Confluence's first destroy mode (the harness's S1m options); random
/// seats tap most of their mana in their upkeep, so they rarely cast either
/// card.
#[test]
fn s1m_battleship_and_confluence_decks_finish_and_replay_through_public_sessions() {
    let colorless = deck(&[
        ("Ancient Tomb", 4),
        ("City of Traitors", 4),
        ("Mishra's Workshop", 4),
        ("Island", 8),
        ("Sol Ring", 4),
        ("Mana Crypt", 4),
        ("Grim Monolith", 4),
        ("Mana Vault", 4),
        (BATTLESHIP, 12),
        ("Reality Smasher", 4),
        ("Thought-Knot Seer", 4),
        ("Wurmcoil Engine", 4),
    ]);
    let red = deck(&[("Mountain", 28), (CONFLUENCE, 12)]);
    let mut totals = DeckEvents::default();
    for (pair, first_seed) in [
        ([colorless.clone(), red.clone()], 0x5331_4d00_u64),
        ([red, colorless], 0x5331_4d10),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                let mut events = DeckEvents::default();
                let game =
                    play_observed(pair.clone(), None, seed, policy, &mut |decision, action| {
                        events.observe(decision, action)
                    });
                let replay = play_observed(pair.clone(), None, seed, policy, &mut |_, _| {});
                assert_eq!(game.transcript_sha256, replay.transcript_sha256);
                let casts = |name: &str| -> usize {
                    game.cast_cards
                        .iter()
                        .map(|cards| cards.get(&id(name)).copied().unwrap_or(0))
                        .sum()
                };
                println!(
                    "seed={seed:#x} random={}: casts {}/{} entry triggers {} stations {} \
                     stationed {} destroy modes {} actions {} winner {:?}",
                    matches!(policy, Policy::Random(_)),
                    casts(BATTLESHIP),
                    casts(CONFLUENCE),
                    events.entry_triggers,
                    events.stations,
                    events.stationed_creatures,
                    events.destroy_modes,
                    game.action_counts.values().sum::<usize>(),
                    game.winner
                );
                totals.entry_triggers += events.entry_triggers;
                totals.stations += events.stations;
                totals.stationed_creatures += events.stationed_creatures;
                totals.destroy_modes += events.destroy_modes;
            }
        }
    }
    println!(
        "entry triggers: {}, stations: {}, stationed decisions: {}, destroy modes: {}",
        totals.entry_triggers, totals.stations, totals.stationed_creatures, totals.destroy_modes
    );
    assert!(
        totals.entry_triggers > 0,
        "no Battleship entry trigger resolved"
    );
    assert!(totals.stations > 0, "no station activation");
    assert!(
        totals.stationed_creatures > 0,
        "no stationed Battleship was a creature in the observation"
    );
    assert!(totals.destroy_modes > 0, "no Fiery Confluence destroy mode");
}
