//! Sideboard wave S1n: native rules tests for Apostle's Blessing, Doorkeeper
//! Thrull, Darksteel Angel, From the Ashes, Kiora's Dismissal and Martyr of
//! Sands, plus public-session consumer games with replay verification.
#![cfg(feature = "eternal-fixtures")]

use crate::eternal_catalog_v1::{play_observed, Policy};
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CardDef, CardType, Keywords, Subtype, CARD_DEFS,
};
use mtg_kernel::creature_types_v60::{canonical_native_type, CreatureTypeV60};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::{CustomCardCountV1, CustomDeckV1};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    GameObject, GameState, ObjectStateV4, PoisonCountersV1, Step, Target, Zone,
};
use serde_json::Value;

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;

const BLESSING: &str = "Apostle's Blessing";
const THRULL: &str = "Doorkeeper Thrull";
const ANGEL: &str = "Darksteel Angel";
const ASHES: &str = "From the Ashes";
const DISMISSAL: &str = "Kiora's Dismissal";
const MARTYR: &str = "Martyr of Sands";

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
        0x5331_4e00_0001,
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

// ---- Card Definitions ---------------------------------------------------------------

#[test]
fn s1n_definitions_and_thrull_subtype_have_printed_characteristics() {
    let blessing = def(BLESSING);
    assert_eq!(blessing.capability, CardCapability::Full);
    assert_eq!(blessing.types, &[CardType::Instant]);
    assert_eq!(blessing.colors, &[ManaColor::W]);
    assert_eq!(blessing.mana_value, 2);
    assert_eq!(blessing.cost.generic, 1);
    assert!(matches!(blessing.cost.pips, [Pip::Phyrexian(ManaColor::W)]));
    assert!(blessing.additional_cost.is_none());

    let thrull = def(THRULL);
    assert_eq!(thrull.capability, CardCapability::Full);
    assert_eq!(thrull.types, &[CardType::Creature]);
    assert_eq!(thrull.subtypes, &[Subtype::Thrull]);
    assert_eq!(Subtype::Thrull.stable_id(), 440);
    assert_eq!(
        canonical_native_type(Subtype::Thrull),
        Some(CreatureTypeV60::Thrull)
    );
    assert_eq!(thrull.power, Some(1));
    assert_eq!(thrull.toughness, Some(2));
    assert!(thrull.keywords.has(Keywords::FLASH));
    assert!(thrull.keywords.has(Keywords::FLYING));

    let angel = def(ANGEL);
    assert_eq!(angel.capability, CardCapability::Full);
    assert_eq!(angel.types, &[CardType::Artifact, CardType::Creature]);
    assert_eq!(angel.subtypes, &[Subtype::Angel]);
    assert_eq!(angel.mana_value, 9);
    assert_eq!(angel.power, Some(4));
    assert_eq!(angel.toughness, Some(4));
    assert!(angel.keywords.has(Keywords::FLYING));
    assert!(angel.keywords.has(Keywords::INDESTRUCTIBLE));
    assert!(angel.game_outcome_protection_v1.is_none());

    let ashes = def(ASHES);
    assert_eq!(ashes.capability, CardCapability::Full);
    assert_eq!(ashes.types, &[CardType::Sorcery]);
    assert_eq!(ashes.colors, &[ManaColor::R]);
    assert_eq!(ashes.mana_value, 4);

    let dismissal = def(DISMISSAL);
    assert_eq!(dismissal.capability, CardCapability::Full);
    assert_eq!(dismissal.types, &[CardType::Instant]);
    assert_eq!(dismissal.colors, &[ManaColor::U]);
    assert_eq!(dismissal.mana_value, 1);

    assert_eq!(def(MARTYR).capability, CardCapability::Full);
}

// ---- Decision-flow helpers ----------------------------------------------------------

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

/// Activates `source`'s ability `index` with the mana already available.
fn activate(state: &mut GameState, source: ObjectId, index: u8) {
    assert!(
        priority(state).2.contains(&(source, index)),
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

/// The current priority holder passes, and `player` (its opponent) then
/// holds priority in the same step.
fn pass_to(state: &mut GameState, player: PlayerId) {
    let (holder, _, _) = priority(state);
    assert_eq!(holder, player.opponent());
    act(state, Action::Pass);
    let (holder, _, _) = priority(state);
    assert_eq!(holder, player);
}

fn life(state: &GameState, player: PlayerId) -> i32 {
    state.players[player.index()].life
}

fn hand_len(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()].hand.len()
}

// ---- Apostle's Blessing -------------------------------------------------------------

/// "Target artifact or creature you control gains protection from artifacts
/// or from the color of your choice until end of turn." With {1} and no
/// white mana, its {W/P} costs 2 life (unpayable at 1 life). Protection from
/// artifacts makes an artifact's ability lose its target (608.2b), drops an
/// attached Equipment (704.5n) and stops artifact creatures from blocking
/// (702.16e).
#[test]
fn apostles_blessing_phyrexian_life_and_artifact_protection_unattach_and_fizzle_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let inspector = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let ring = put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        let opp_inspector = put(&mut state, opponent, "Thraben Inspector", Zone::Battlefield);
        let spear = put(&mut state, actor, "Shadowspear", Zone::Battlefield);
        let spellbomb = put(&mut state, opponent, "Aether Spellbomb", Zone::Battlefield);
        let blessing = put(&mut state, actor, BLESSING, Zone::Hand);

        // Shadowspear's equip {2} (its second ability) onto the Inspector.
        pool(&mut state, actor, &[], 2);
        activate(&mut state, spear, 1);
        choose_target(&mut state, Target::Object(inspector));
        settled(&mut state);
        assert!(state.objects.get(inspector).attachments.contains(&spear));

        // {W/P} without white mana: 2 life, so not at 1 life (119.4).
        state.players[actor.index()].life = 1;
        pool(&mut state, actor, &[], 1);
        assert!(
            !priority(&mut state).1.contains(&blessing),
            "1 life and no white mana cannot pay {{W/P}}"
        );
        state.players[actor.index()].life = 4;
        assert!(priority(&mut state).1.contains(&blessing));

        // The opponent activates Aether Spellbomb ("{U}, Sacrifice: Return
        // target creature to its owner's hand") at the Inspector, and the
        // actor answers with Apostle's Blessing for {1} and 2 life.
        pool(&mut state, opponent, &[ManaColor::U], 0);
        pass_to(&mut state, opponent);
        activate(&mut state, spellbomb, 0);
        choose_target(&mut state, Target::Object(inspector));
        // Costs are paid once the engine completes the activation.
        assert_eq!(priority(&mut state).0, opponent);
        assert_eq!(zone(&state, spellbomb), Zone::Graveyard);
        pass_to(&mut state, actor);
        cast(&mut state, blessing);
        let legal = choose_target(&mut state, Target::Object(inspector));
        // Artifacts and creatures the actor controls; nothing the opponent
        // controls.
        assert!(legal.contains(&Target::Object(ring)));
        assert!(legal.contains(&Target::Object(spear)));
        assert!(!legal.contains(&Target::Object(opp_inspector)));
        assert_eq!(priority(&mut state).0, actor);
        assert_eq!(life(&state, actor), 2);

        let Some(Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        }) = settle(&mut state)
        else {
            panic!("protection quality choice");
        };
        assert_eq!(player, actor);
        // Artifacts, then white, blue, black, red and green.
        assert_eq!(option_count, 6);
        act(&mut state, Action::ChooseEffectOption(0));
        settled(&mut state);

        // The Spellbomb's ability lost its only target, and Shadowspear fell
        // off as a state-based action.
        assert_eq!(zone(&state, inspector), Zone::Battlefield);
        assert!(state.objects.get(inspector).attachments.is_empty());
        assert_eq!(zone(&state, spear), Zone::Battlefield);

        // An artifact creature can't block the Inspector this turn; a
        // nonartifact creature can.
        let enforcer = put(&mut state, opponent, "Myr Enforcer", Zone::Battlefield);
        loop {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::DeclareAttackers { player, eligible } => {
                    assert_eq!(player, actor);
                    assert!(eligible.contains(&inspector));
                    act(&mut state, Action::DeclareAttackers(vec![inspector]));
                    break;
                }
                other => panic!("expected attacks, got {other:?}"),
            }
        }
        let legal_blockers = loop {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::DeclareBlockers {
                    player,
                    legal_blockers,
                    ..
                } => {
                    assert_eq!(player, opponent);
                    break legal_blockers;
                }
                other => panic!("expected blocks, got {other:?}"),
            }
        };
        let (_, blockers) = legal_blockers
            .iter()
            .find(|(attacker, _)| *attacker == inspector)
            .expect("the Inspector attacks");
        assert!(blockers.contains(&opp_inspector));
        assert!(
            !blockers.contains(&enforcer),
            "an artifact creature can't block a creature with protection from artifacts"
        );
    }
}

/// Protection from a chosen color: with {W} available the spell costs no
/// life, Lightning Bolt can't target the creature this turn, and the
/// protection ends with the turn.
#[test]
fn apostles_blessing_color_protection_blocks_red_targeting_and_expires_next_turn() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        let ring = put(&mut state, actor, "Sol Ring", Zone::Battlefield);
        let thrull = put(&mut state, actor, THRULL, Zone::Battlefield);
        let blessing = put(&mut state, actor, BLESSING, Zone::Hand);
        let bolt_now = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);
        let bolt_next = put(&mut state, opponent, "Lightning Bolt", Zone::Hand);

        pool(&mut state, actor, &[ManaColor::W], 1);
        cast(&mut state, blessing);
        let legal = choose_target(&mut state, Target::Object(thrull));
        assert!(legal.contains(&Target::Object(ring)));
        assert_eq!(priority(&mut state).0, actor);
        assert_eq!(life(&state, actor), 20, "{{W}} paid the {{W/P}}");
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::W.pool_index()],
            0
        );

        let Some(Decision::ChooseEffectOption { option_count, .. }) = settle(&mut state) else {
            panic!("protection quality choice");
        };
        assert_eq!(option_count, 6);
        act(&mut state, Action::ChooseEffectOption(4)); // Red.
        settled(&mut state);

        // The opponent's Bolt can't target the Thrull this turn.
        pool(&mut state, opponent, &[ManaColor::R], 0);
        pass_to(&mut state, opponent);
        cast(&mut state, bolt_now);
        let legal = choose_target(&mut state, Target::Player(actor));
        assert!(!legal.contains(&Target::Object(thrull)));
        settled(&mut state);
        assert_eq!(life(&state, actor), 17);
        assert_eq!(zone(&state, thrull), Zone::Battlefield);

        // On the opponent's turn the protection has ended.
        loop {
            let decision = next(&mut state);
            if state.active_player == opponent
                && state.step == Step::Main1
                && state.stack.is_empty()
                && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == opponent)
            {
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(Vec::new()))
                }
                Decision::Discard { count, choices, .. } => act(
                    &mut state,
                    Action::Discard(choices[..count as usize].to_vec()),
                ),
                other => panic!("unexpected decision: {other:?}"),
            }
        }
        pool(&mut state, opponent, &[ManaColor::R], 0);
        cast(&mut state, bolt_next);
        choose_target(&mut state, Target::Object(thrull));
        settled(&mut state);
        assert_eq!(zone(&state, thrull), Zone::Graveyard);
    }
}

// ---- Doorkeeper Thrull --------------------------------------------------------------

/// "Flash. Flying. Artifacts and creatures entering the battlefield don't
/// cause abilities to trigger." Entry triggers of creatures and artifacts
/// (including one entering with the Thrull) don't trigger; a noncreature,
/// nonartifact permanent's entry trigger and an as-enters replacement still
/// apply.
#[test]
fn doorkeeper_thrull_suppresses_artifact_and_creature_etbs_but_keeps_enchantment_etb_and_entry_replacements(
) {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        // Flashed in over the opponent's Baleful Strix, the Thrull stops the
        // Strix's and then Coveted Jewel's draws.
        let mut state = game(opponent);
        let thrull = put(&mut state, actor, THRULL, Zone::Hand);
        let strix = put(&mut state, opponent, "Baleful Strix", Zone::Hand);
        let jewel = put(&mut state, opponent, "Coveted Jewel", Zone::Hand);
        pool(&mut state, opponent, &[ManaColor::U, ManaColor::B], 6);
        cast(&mut state, strix);
        pool(&mut state, actor, &[ManaColor::W], 1);
        pass_to(&mut state, actor);
        cast(&mut state, thrull);
        settled(&mut state);
        assert_eq!(zone(&state, thrull), Zone::Battlefield);
        assert_eq!(zone(&state, strix), Zone::Battlefield);
        assert_eq!(hand_len(&state, opponent), 1, "the Strix drew nothing");
        cast(&mut state, jewel);
        settled(&mut state);
        assert_eq!(zone(&state, jewel), Zone::Battlefield);
        assert_eq!(hand_len(&state, opponent), 0, "the Jewel drew nothing");

        // Control: without the Thrull, the Strix's entry draws a card.
        let mut state = game(actor);
        let strix = put(&mut state, opponent, "Baleful Strix", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![ProposedEvent::zone_change(strix, Zone::Battlefield)],
        );
        settled(&mut state);
        assert_eq!(hand_len(&state, opponent), 1);

        // Entering together with the Thrull, the Strix draws nothing.
        let mut state = game(actor);
        let thrull = put(&mut state, actor, THRULL, Zone::Hand);
        let strix = put(&mut state, opponent, "Baleful Strix", Zone::Hand);
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::zone_change(thrull, Zone::Battlefield),
                ProposedEvent::zone_change(strix, Zone::Battlefield),
            ],
        );
        settled(&mut state);
        assert_eq!(zone(&state, thrull), Zone::Battlefield);
        assert_eq!(zone(&state, strix), Zone::Battlefield);
        assert_eq!(hand_len(&state, opponent), 0);

        // Spreading Seas (an enchantment) still draws, and Moonshadow still
        // enters with six -1/-1 counters (614.1c).
        let mut state = game(actor);
        put(&mut state, actor, THRULL, Zone::Battlefield);
        let land = put(&mut state, opponent, "Mountain", Zone::Battlefield);
        let seas = put(&mut state, actor, "Spreading Seas", Zone::Hand);
        let moonshadow = put(&mut state, actor, "Moonshadow", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::B], 1);
        cast(&mut state, seas);
        choose_target(&mut state, Target::Object(land));
        settled(&mut state);
        assert_eq!(zone(&state, seas), Zone::Battlefield);
        assert_eq!(hand_len(&state, actor), 2, "Moonshadow and the Seas draw");
        cast(&mut state, moonshadow);
        settled(&mut state);
        assert_eq!(zone(&state, moonshadow), Zone::Battlefield);
        assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 6);
    }
}

// ---- Darksteel Angel ----------------------------------------------------------------

/// "You can't lose the game and your opponents can't win the game.
/// Creatures you control can't have -1/-1 counters put on them." Losing its
/// abilities (Dress Down) lets state-based actions end the game.
#[test]
fn darksteel_angel_prevents_loss_opponent_oracle_win_and_minus1_counters_until_disabled() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        // Life below zero, ten poison counters and a draw from an empty
        // library don't make its controller lose.
        let mut state = game(actor);
        put(&mut state, actor, ANGEL, Zone::Battlefield);
        state.players[actor.index()].life = -3;
        state.players[actor.index()].poison_counters = PoisonCountersV1(10);
        state.players[actor.index()].library.clear();
        event::propose_and_commit(&mut state, ProposedEvent::draw(actor));
        let (holder, _, _) = priority(&mut state);
        assert_eq!(holder, actor);
        assert!(state.players.iter().all(|player| !player.has_lost));

        // The opponent's Thassa's Oracle over an empty library wins without
        // the Angel and doesn't win with it.
        for angel in [false, true] {
            let mut state = game(opponent);
            if angel {
                put(&mut state, actor, ANGEL, Zone::Battlefield);
            }
            state.players[opponent.index()].library.clear();
            let oracle = put(&mut state, opponent, "Thassa's Oracle", Zone::Hand);
            pool(&mut state, opponent, &[ManaColor::U, ManaColor::U], 0);
            cast(&mut state, oracle);
            let outcome = loop {
                match settle(&mut state) {
                    None => break None,
                    Some(Decision::ChooseEffectTargets { .. }) => {
                        act(&mut state, Action::FinishEffectSelection)
                    }
                    Some(Decision::GameOver { winner }) => break Some(winner),
                    Some(other) => panic!("unexpected decision: {other:?}"),
                }
            };
            if angel {
                assert_eq!(outcome, None);
                assert!(state.players.iter().all(|player| !player.has_lost));
            } else {
                assert_eq!(outcome, Some(Some(opponent)));
            }
        }

        // Fume Spitter ("Sacrifice: Put a -1/-1 counter on target creature")
        // can't put a counter on the Angel controller's creature, but can on
        // the opponent's, and Moonshadow enters without its six counters.
        let mut state = game(actor);
        put(&mut state, actor, ANGEL, Zone::Battlefield);
        let ally = put(&mut state, actor, "Thraben Inspector", Zone::Battlefield);
        let opp_ally = put(&mut state, opponent, "Thraben Inspector", Zone::Battlefield);
        let spitter_a = put(&mut state, opponent, "Fume Spitter", Zone::Battlefield);
        let spitter_b = put(&mut state, actor, "Fume Spitter", Zone::Battlefield);
        let moonshadow = put(&mut state, actor, "Moonshadow", Zone::Hand);
        pass_to(&mut state, opponent);
        activate(&mut state, spitter_a, 0);
        choose_target(&mut state, Target::Object(ally));
        settled(&mut state);
        assert_eq!(zone(&state, spitter_a), Zone::Graveyard);
        assert_eq!(state.objects.get(ally).counters.minus1_minus1, 0);
        activate(&mut state, spitter_b, 0);
        choose_target(&mut state, Target::Object(opp_ally));
        settled(&mut state);
        assert_eq!(state.objects.get(opp_ally).counters.minus1_minus1, 1);
        pool(&mut state, actor, &[ManaColor::B], 0);
        cast(&mut state, moonshadow);
        settled(&mut state);
        assert_eq!(zone(&state, moonshadow), Zone::Battlefield);
        assert_eq!(state.objects.get(moonshadow).counters.minus1_minus1, 0);
        assert_eq!(engine::effective_power(&state, moonshadow), 7);

        // Dress Down removes the Angel's abilities: the controller at 0 life
        // with ten poison counters loses at once.
        let mut state = game(actor);
        let angel = put(&mut state, actor, ANGEL, Zone::Battlefield);
        let dress_down = put(&mut state, opponent, "Dress Down", Zone::Hand);
        state.players[actor.index()].life = 0;
        state.players[actor.index()].poison_counters = PoisonCountersV1(10);
        assert_eq!(priority(&mut state).0, actor);
        assert!(!state.players[actor.index()].has_lost);
        pool(&mut state, opponent, &[ManaColor::U], 1);
        pass_to(&mut state, opponent);
        cast(&mut state, dress_down);
        let Some(Decision::GameOver { winner }) = settle(&mut state) else {
            panic!("the game ends once the Angel loses its abilities");
        };
        assert_eq!(winner, Some(opponent));
        assert_eq!(zone(&state, angel), Zone::Battlefield);
        assert!(state.players[actor.index()].has_lost);
        assert!(!state.players[opponent.index()].has_lost);
    }
}

// ---- From the Ashes -----------------------------------------------------------------

#[test]
fn from_the_ashes_destroys_nonbasics_under_blood_moon_spares_indestructible_and_orders_apnap_searches(
) {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = game(actor);
        state.players[actor.index()].library.clear();
        state.players[opponent.index()].library.clear();

        let actor_bolt = put(&mut state, actor, "Lightning Bolt", Zone::Library);
        let actor_forest = put(&mut state, actor, "Forest", Zone::Library);
        let actor_plains = put(&mut state, actor, "Plains", Zone::Library);
        let _opp_card_a = put(&mut state, opponent, "Lightning Bolt", Zone::Library);
        let _opp_card_b = put(&mut state, opponent, "Seal of Fire", Zone::Library);
        let _opp_island = put(&mut state, opponent, "Island", Zone::Library);
        let opp_order_before = state.players[opponent.index()].library.clone();

        // Actor controls 2 nonbasic lands (still nonbasic under Blood Moon),
        // 1 basic Mountain, and 1 indestructible Rustvale Bridge.
        let tomb = put(&mut state, actor, "Ancient Tomb", Zone::Battlefield);
        let volc = put(&mut state, actor, "Volcanic Island", Zone::Battlefield);
        let basic_mtn = put(&mut state, actor, "Mountain", Zone::Battlefield);
        let bridge = put(&mut state, actor, "Rustvale Bridge", Zone::Battlefield);
        let opp_volc = put(&mut state, opponent, "Volcanic Island", Zone::Battlefield);
        let _moon = put(&mut state, opponent, "Blood Moon", Zone::Battlefield);
        let ashes = put(&mut state, actor, ASHES, Zone::Hand);

        pool(&mut state, actor, &[ManaColor::R], 3);
        let _ = next(&mut state);
        act(&mut state, Action::CastSpell(ashes));

        // Active player (`actor`) is asked first in APNAP order.
        let Some(Decision::ChooseEffectOption {
            player,
            option_count,
            ..
        }) = settle(&mut state)
        else {
            panic!("active player choice");
        };
        assert_eq!(player, actor);
        assert_eq!(option_count, 2);
        assert_eq!(zone(&state, tomb), Zone::Graveyard);
        assert_eq!(zone(&state, volc), Zone::Graveyard);
        assert_eq!(zone(&state, opp_volc), Zone::Graveyard);
        assert_eq!(zone(&state, basic_mtn), Zone::Battlefield);
        assert_eq!(zone(&state, bridge), Zone::Battlefield);

        // Actor chooses to search (option 0): max_targets == 2 (Bridge survived).
        act(&mut state, Action::ChooseEffectOption(0));
        let Decision::ChooseEffectTargets {
            player: search_player,
            legal_targets,
            max_targets,
            ..
        } = next(&mut state)
        else {
            panic!("actor library search pick 1");
        };
        assert_eq!(search_player, actor);
        assert_eq!(max_targets, 2);
        assert!(legal_targets.contains(&Target::Object(actor_plains)));
        assert!(legal_targets.contains(&Target::Object(actor_forest)));
        assert!(!legal_targets.contains(&Target::Object(actor_bolt)));
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(actor_plains)),
        );
        if let Decision::ChooseEffectTargets { legal_targets, .. } = next(&mut state) {
            assert!(legal_targets.contains(&Target::Object(actor_forest)));
            act(
                &mut state,
                Action::ChooseEffectTarget(Target::Object(actor_forest)),
            );
        }

        // Nonactive player (`opponent`) is asked second and declines (option 1).
        let Some(Decision::ChooseEffectOption {
            player: second_player,
            option_count: second_count,
            ..
        }) = settle(&mut state)
        else {
            panic!("nonactive player choice");
        };
        assert_eq!(second_player, opponent);
        assert_eq!(second_count, 2);
        act(&mut state, Action::ChooseEffectOption(1)); // Decline search -> no shuffle.
        settled(&mut state);

        assert_eq!(zone(&state, actor_plains), Zone::Battlefield);
        assert!(!state.objects.get(actor_plains).tapped);
        assert_eq!(zone(&state, actor_forest), Zone::Battlefield);
        assert!(!state.objects.get(actor_forest).tapped);
        assert_eq!(state.players[opponent.index()].library, opp_order_before);
    }
}

// ---- Kiora's Dismissal --------------------------------------------------------------

/// Offers `target` at the pending variable target prompt, which must still
/// allow finishing.
fn choose_more(state: &mut GameState, target: Target) -> Vec<Target> {
    match next(state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish);
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

/// Finishes the variable target prompt if it is still open, returning the
/// targets it still offered.
fn finish_targets(state: &mut GameState) -> Vec<Target> {
    match next(state) {
        Decision::ChooseTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            assert!(can_finish);
            act(state, Action::FinishEffectSelection);
            legal_targets
        }
        Decision::CastSpellOrPass { .. } => Vec::new(),
        other => panic!("expected a target prompt or priority, got {other:?}"),
    }
}

fn blue(state: &GameState, player: PlayerId) -> u8 {
    state.players[player.index()].mana_pool[ManaColor::U.pool_index()]
}

/// "Strive -- This spell costs {U} more to cast for each target beyond the
/// first. Return any number of target enchantments to their owners' hands."
/// Zero targets are legal; a target the caster can't pay for isn't offered;
/// a target that becomes illegal is left alone while the others return
/// (608.2b); casting without paying the mana cost (Omniscience) still pays
/// the strive cost.
#[test]
fn kioras_dismissal_strive_payability_omniscience_and_partial_fizzle_in_both_seats() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();

        // With {U}{U}: two targets, and no third.
        let mut state = game(actor);
        let seal_a = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_b = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_c = put(&mut state, actor, "Seal of Fire", Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 0);
        cast(&mut state, dismissal);
        let first = choose_more(&mut state, Target::Object(seal_a));
        assert_eq!(first.len(), 3, "{first:?}");
        let second = choose_more(&mut state, Target::Object(seal_b));
        assert_eq!(second.len(), 2, "{second:?}");
        let third = finish_targets(&mut state);
        assert!(
            !third.contains(&Target::Object(seal_c)),
            "a third target costs {{U}}{{U}}{{U}}"
        );
        assert_eq!(priority(&mut state).0, actor);
        assert_eq!(blue(&state, actor), 0, "{{U}} plus one strive {{U}}");
        settled(&mut state);
        assert_eq!(zone(&state, seal_a), Zone::Hand);
        assert_eq!(zone(&state, seal_b), Zone::Hand);
        assert_eq!(zone(&state, seal_c), Zone::Battlefield);

        // With {U}: one target, and the spell may also finish with none.
        let mut state = game(actor);
        let seal_a = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_b = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 0);
        cast(&mut state, dismissal);
        choose_more(&mut state, Target::Object(seal_a));
        assert!(!finish_targets(&mut state).contains(&Target::Object(seal_b)));
        settled(&mut state);
        assert_eq!(zone(&state, seal_a), Zone::Hand);
        assert_eq!(zone(&state, seal_b), Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 0);
        cast(&mut state, dismissal);
        finish_targets(&mut state);
        settled(&mut state);
        assert_eq!(zone(&state, dismissal), Zone::Graveyard);
        assert_eq!(zone(&state, seal_b), Zone::Battlefield);

        // Veil of Summer gives the opponent's Seal hexproof from blue in
        // response: the actor's own Seal still returns.
        let mut state = game(actor);
        let seal_c = put(&mut state, actor, "Seal of Fire", Zone::Battlefield);
        let seal_d = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);
        let veil = put(&mut state, opponent, "Veil of Summer", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U, ManaColor::U], 0);
        pool(&mut state, opponent, &[ManaColor::G], 0);
        cast(&mut state, dismissal);
        choose_more(&mut state, Target::Object(seal_d));
        choose_more(&mut state, Target::Object(seal_c));
        finish_targets(&mut state);
        pass_to(&mut state, opponent);
        cast(&mut state, veil);
        settled(&mut state);
        assert_eq!(zone(&state, veil), Zone::Graveyard);
        assert_eq!(zone(&state, seal_d), Zone::Battlefield);
        assert_eq!(zone(&state, seal_c), Zone::Hand);
        assert_eq!(zone(&state, dismissal), Zone::Graveyard);

        // Under Omniscience with {U}: cast without paying its mana cost, the
        // second target still costs {U} and a third is unpayable.
        let mut state = game(actor);
        put(&mut state, actor, "Omniscience", Zone::Battlefield);
        let seal_d = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_e = put(&mut state, opponent, "Seal of Fire", Zone::Battlefield);
        let seal_f = put(&mut state, actor, "Seal of Fire", Zone::Battlefield);
        let dismissal = put(&mut state, actor, DISMISSAL, Zone::Hand);
        pool(&mut state, actor, &[ManaColor::U], 0);
        cast(&mut state, dismissal);
        choose_more(&mut state, Target::Object(seal_d));
        choose_more(&mut state, Target::Object(seal_e));
        assert!(!finish_targets(&mut state).contains(&Target::Object(seal_f)));
        // With {U} floating, Omniscience still offers paying the mana cost.
        match next(&mut state) {
            Decision::ChooseEffectOption { option_count, .. } => {
                assert_eq!(option_count, 2);
                act(&mut state, Action::ChooseEffectOption(1)); // Without paying.
            }
            other => panic!("expected the free-cast choice, got {other:?}"),
        }
        assert_eq!(priority(&mut state).0, actor);
        assert_eq!(blue(&state, actor), 0, "the strive {{U}}");
        settled(&mut state);
        assert_eq!(zone(&state, seal_d), Zone::Hand);
        assert_eq!(zone(&state, seal_e), Zone::Hand);
        assert_eq!(zone(&state, seal_f), Zone::Battlefield);
    }
}

// ---- Martyr of Sands + S1n Public-Session Consumer Games ----------------------------

#[derive(Default)]
struct S1nSessionEvents {
    martyr_activations: usize,
    blessing_choices: usize,
    ashes_choices: usize,
}

impl S1nSessionEvents {
    fn observe(&mut self, _decision: &Value, action: &Value) {
        let semantic = &action["semantic"];
        let card = semantic["source"]["card_db_id"]
            .as_u64()
            .map(|id| CARD_DEFS[id as usize].name);
        match semantic["action_kind"].as_str() {
            Some("activate_ability") if card == Some(MARTYR) => {
                self.martyr_activations += 1;
            }
            Some("choose_effect_option") if card == Some(BLESSING) => {
                self.blessing_choices += 1;
            }
            Some("choose_effect_option") if card == Some(ASHES) => {
                self.ashes_choices += 1;
            }
            _ => {}
        }
    }
}

#[test]
fn s1n_and_martyr_of_sands_decks_finish_and_replay_through_public_sessions() {
    let white_artifacts = deck(&[
        ("Plains", 10),
        ("Ancient Tomb", 4),
        ("Sol Ring", 4),
        ("Mana Vault", 4),
        (MARTYR, 6),
        (THRULL, 5),
        (BLESSING, 5),
        (ANGEL, 2),
    ]);
    let izzet_ashes = deck(&[
        ("Island", 10),
        ("Mountain", 8),
        ("Volcanic Island", 4),
        ("Spreading Seas", 6),
        (DISMISSAL, 6),
        (ASHES, 6),
    ]);

    let mut totals = S1nSessionEvents::default();
    let mut cast_totals = [0usize; 6];
    let tracked = [MARTYR, THRULL, BLESSING, ANGEL, DISMISSAL, ASHES];

    for (pair, first_seed) in [
        (
            [white_artifacts.clone(), izzet_ashes.clone()],
            0x5331_4e00_u64,
        ),
        ([izzet_ashes, white_artifacts], 0x5331_4e10),
    ] {
        for seed in first_seed..first_seed + 3 {
            for policy in [
                Policy::Ranked,
                Policy::Random(seed + 0x100),
                Policy::Random(seed + 0x200),
            ] {
                let mut events = S1nSessionEvents::default();
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
                totals.martyr_activations += events.martyr_activations;
                totals.blessing_choices += events.blessing_choices;
                totals.ashes_choices += events.ashes_choices;
            }
        }
    }

    assert!(cast_totals[0] > 0, "no Martyr of Sands cast");
    assert!(cast_totals[1] > 0, "no Doorkeeper Thrull cast");
    assert!(cast_totals[2] > 0, "no Apostle's Blessing cast");
    assert!(cast_totals[4] > 0, "no Kiora's Dismissal cast");
    assert!(cast_totals[5] > 0, "no From the Ashes cast");
    assert!(
        totals.martyr_activations > 0,
        "no Martyr of Sands activation"
    );
    assert!(
        totals.blessing_choices > 0,
        "no Apostle's Blessing option choice"
    );
    assert!(totals.ashes_choices > 0, "no From the Ashes option choice");
}
