//! CR 506.4 / 511.3: a permanent that leaves the battlefield is removed from
//! combat, and every creature stops attacking or blocking as the end of
//! combat step ends. mtg-kernel kept departed objects (and last turn's
//! attackers) in its combat record (found by shadowing mtg-kernel with the
//! gorge engine in SpellBench Faeries / Burn mirrors).

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl::observe_v2;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::HarnessSurfaceV2;

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_game(seed: u64) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.active_player = PlayerId::P0;
    state.priority_player = PlayerId::P0;
    state.step = Step::Main1;
    state
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.to_string(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Counters::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(object),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(object),
        Zone::Library => state.players[owner.index()].library.push(object),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Command => state.command.push(object),
        Zone::Stack => panic!("test helper does not fabricate stack-zone objects"),
    }
    object
}

/// Passes priority until `done` holds, failing loudly on a halt or on any
/// non-priority decision.
fn pass_until(state: &mut GameState, done: impl Fn(&GameState) -> bool) -> Decision {
    for _ in 0..32 {
        let decision = engine::advance_until_decision(state);
        if let Decision::Halted { mechanic, source } = decision {
            panic!("unexpected halt {mechanic:?} from {source}");
        }
        if done(state) {
            return decision;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected decision while passing: {other:?}"),
        }
    }
    panic!("condition did not become true within bounded priority walk")
}

/// Activates `ninja`'s ninjutsu from hand and pays it by returning exactly
/// `returned`, the only unblocked attacker the controller has.
fn activate_ninjutsu(state: &mut GameState, ninja: ObjectId, returned: ObjectId) {
    let Decision::CastSpellOrPass {
        activatable_abilities,
        ..
    } = engine::advance_until_decision(state)
    else {
        panic!("priority decision before ninjutsu")
    };
    assert!(
        activatable_abilities.contains(&(ninja, 0)),
        "ninjutsu of {ninja} is offered"
    );
    engine::step(state, Action::ActivateAbility(ninja, 0)).unwrap();
    // A single legal attacker to return is paid without a choice.
    match engine::advance_until_decision(state) {
        Decision::ChooseCostTargets {
            cost_kind,
            candidates,
            ..
        } => {
            assert_eq!(cost_kind, CostKind::ReturnPermanentsToHand);
            assert_eq!(candidates, vec![returned]);
            engine::step(state, Action::ChooseCostTarget(returned)).unwrap();
        }
        Decision::CastSpellOrPass { .. } => {}
        other => panic!("ninjutsu return cost: {other:?}"),
    }
    assert_eq!(state.objects.get(returned).zone, Zone::Hand);
    assert_eq!(state.objects.get(ninja).zone, Zone::Hand);
    assert!(state.stack.last().is_some_and(|item| item.source == ninja));
}

fn combat_game(seed: u64) -> GameState {
    let mut state = ready_game(seed);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    state
}

fn attack_with(state: &mut GameState, name: &str) -> ObjectId {
    let attacker = put_object(state, PlayerId::P0, name, Zone::Battlefield);
    state.objects.get_mut(attacker).tapped = true;
    state.engine.combat.attackers.push(attacker);
    attacker
}

fn projected_attackers(state: &GameState) -> Vec<u32> {
    observe_v2(state, &HarnessSurfaceV2::new(), PlayerId::P0, 0)
        .expect("observation")
        .projection
        .combat
        .ordered_attackers
        .iter()
        .map(|attacker| attacker.arena_id)
        .collect()
}

/// Faeries-800 step 43: ninjutsu returned Faerie Seer to hand, yet the
/// public combat record still listed it (zone Hand) beside the Ninja.
#[test]
fn an_attacker_returned_by_ninjutsu_is_removed_from_combat() {
    let mut state = combat_game(0x4b53_4841_444f_5703);
    let seer = attack_with(&mut state, "Faerie Seer");
    let ninja = put_object(
        &mut state,
        PlayerId::P0,
        "Ninja of the Deep Hours",
        Zone::Hand,
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 2;

    activate_ninjutsu(&mut state, ninja, seer);
    assert!(
        !state.engine.combat.attackers.contains(&seer),
        "the returned Seer left the battlefield and is no longer attacking"
    );
    pass_until(&mut state, |state| {
        state.stack.is_empty() && state.objects.get(ninja).zone == Zone::Battlefield
    });
    assert_eq!(state.engine.combat.attackers, vec![ninja]);
    assert_eq!(projected_attackers(&state), vec![ninja.0]);
}

/// A creature that leaves the battlefield and comes back mid-combat is a
/// new object (CR 400.7) that was never declared as an attacker, so it deals
/// no combat damage.
#[test]
fn a_creature_that_leaves_and_returns_mid_combat_is_not_attacking() {
    let mut state = combat_game(0x4b53_4841_444f_5704);
    let seer = attack_with(&mut state, "Faerie Seer");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(seer, Zone::Hand));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(seer, Zone::Battlefield),
    );
    assert_eq!(state.objects.get(seer).zone, Zone::Battlefield);
    assert!(!state.engine.combat.attackers.contains(&seer));
    assert!(projected_attackers(&state).is_empty());

    let life_before = state.players[1].life;
    pass_until(&mut state, |state| state.step == Step::CombatDamage);
    assert_eq!(state.players[1].life, life_before);
}

/// Burn-800 step 28: a blocker that died stayed in the combat record. The
/// attacker it blocked remains blocked (CR 509.1h) with no blockers, and
/// every creature leaves combat as the end of combat step ends (CR 511.3):
/// Faeries-800 step 250 still showed last turn's attackers in a later main
/// phase.
#[test]
fn a_dead_blocker_leaves_combat_and_combat_ends_with_the_end_of_combat_step() {
    let mut state = combat_game(0x4b53_4841_444f_5705);
    let attacker = attack_with(&mut state, "Faerie Seer");
    let blocker = put_object(
        &mut state,
        PlayerId::P1,
        "Faerie Miscreant",
        Zone::Battlefield,
    );
    state.engine.combat.blocked_by = vec![(attacker, vec![blocker])];

    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(blocker, Zone::Graveyard),
    );
    assert_eq!(
        state.engine.combat.blocked_by,
        vec![(attacker, Vec::new())],
        "the attacker stays blocked by no creature"
    );
    let observation = observe_v2(&state, &HarnessSurfaceV2::new(), PlayerId::P0, 0).unwrap();
    assert!(observation
        .projection
        .combat
        .attacker_to_ordered_blockers
        .iter()
        .all(|(_, blockers)| blockers.is_empty()));

    let life_before = state.players[1].life;
    pass_until(&mut state, |state| state.step == Step::Main2);
    assert_eq!(
        state.players[1].life, life_before,
        "a blocked attacker deals no player damage"
    );
    assert!(state.engine.combat.attackers.is_empty());
    assert!(state.engine.combat.blocked_by.is_empty());
    assert!(projected_attackers(&state).is_empty());
}

/// Tinder Wall's "{R}, Sacrifice Tinder Wall: It deals 2 damage to target
/// creature it's blocking" read the combat record after combat had ended,
/// so it could still target a creature it blocked earlier this turn.
#[test]
fn a_blocker_is_not_blocking_after_the_end_of_combat_step() {
    let mut state = ready_game(0x4b53_4841_444f_570d);
    state.active_player = PlayerId::P1;
    state.priority_player = PlayerId::P1;
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let attacker = put_object(&mut state, PlayerId::P1, "Faerie Seer", Zone::Battlefield);
    state.objects.get_mut(attacker).tapped = true;
    state.engine.combat.attackers = vec![attacker];
    let wall = put_object(&mut state, PlayerId::P0, "Tinder Wall", Zone::Battlefield);
    state.engine.combat.blocked_by = vec![(attacker, vec![wall])];

    let offers_wall = |state: &mut GameState| {
        state.players[0].mana_pool[ManaColor::R.pool_index()] = 1;
        let Decision::CastSpellOrPass {
            player,
            activatable_abilities,
            ..
        } = engine::advance_until_decision(state)
        else {
            panic!("priority decision")
        };
        assert_eq!(player, PlayerId::P0);
        activatable_abilities.contains(&(wall, 0))
    };

    engine::step(&mut state, Action::Pass).unwrap();
    assert!(offers_wall(&mut state), "the Wall is blocking the Seer");

    engine::step(&mut state, Action::Pass).unwrap();
    pass_until(&mut state, |state| {
        state.step == Step::Main2 && state.priority_player == PlayerId::P0
    });
    assert_eq!(state.objects.get(attacker).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(wall).zone, Zone::Battlefield);
    assert!(
        !offers_wall(&mut state),
        "after combat the Wall blocks nothing, so its ability has no target"
    );
}
