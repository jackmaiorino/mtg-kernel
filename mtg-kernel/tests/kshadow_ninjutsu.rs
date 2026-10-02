//! A second ninjutsu activation in one combat halted the game with
//! `InvalidEffectContinuation` (found by shadowing mtg-kernel with the gorge
//! engine in SpellBench Faeries mirrors).

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};

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

/// CR 702.49a-c: ninjutsu has no once-per-combat limit. Moon-Circuit Hacker
/// attacks unblocked, Ninja of the Deep Hours' ninjutsu returns it, and then
/// the returned Hacker's own ninjutsu returns the Ninja. The Hacker comes
/// back as a new object (CR 400.7) that is attacking and unblocked; its
/// previous incarnation's stale `combat.attackers` entry must not halt the
/// resolution or make it attack twice.
#[test]
fn a_second_ninjutsu_in_one_combat_can_put_the_returned_attacker_back() {
    let mut state = ready_game(0x4b53_4841_444f_5701);
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers_declared = true;
    state.engine.combat.blockers_declared = true;
    let hacker = put_object(
        &mut state,
        PlayerId::P0,
        "Moon-Circuit Hacker",
        Zone::Battlefield,
    );
    state.objects.get_mut(hacker).tapped = true;
    state.engine.combat.attackers = vec![hacker];
    let ninja = put_object(
        &mut state,
        PlayerId::P0,
        "Ninja of the Deep Hours",
        Zone::Hand,
    );
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 3;

    activate_ninjutsu(&mut state, ninja, hacker);
    pass_until(&mut state, |state| {
        state.stack.is_empty() && state.objects.get(ninja).zone == Zone::Battlefield
    });
    assert!(state.engine.combat.attackers.contains(&ninja));

    activate_ninjutsu(&mut state, hacker, ninja);
    pass_until(&mut state, |state| {
        state.stack.is_empty() && state.objects.get(hacker).zone == Zone::Battlefield
    });
    assert!(state.engine.halted.is_none());
    assert!(state.objects.get(hacker).tapped);
    assert_eq!(state.objects.get(ninja).zone, Zone::Hand);
    assert_eq!(
        state
            .engine
            .combat
            .attackers
            .iter()
            .filter(|&&attacker| attacker == hacker)
            .count(),
        1,
        "the re-entered Hacker attacks exactly once"
    );
    assert!(state
        .engine
        .combat
        .blocked_by
        .iter()
        .all(|(attacker, _)| *attacker != hacker));

    let power = engine::effective_power(&state, hacker);
    assert!(power > 0);
    let life_before = state.players[1].life;
    pass_until(&mut state, |state| state.step == Step::CombatDamage);
    assert_eq!(state.players[1].life, life_before - power);
}
