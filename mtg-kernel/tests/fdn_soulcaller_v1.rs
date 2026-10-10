//! Prepared v69 card cases; execution requires serial Vampire Soulcaller admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, Keywords, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::combat_damage_v1::enable_foundations_combat_v1;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use mtg_kernel::trigger;

fn ready(player: PlayerId) -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[plains; 40],
        &[plains; 40],
        |_| "Plains".into(),
        690,
        player,
    );
    state.step = Step::Main1;
    enable_foundations_combat_v1(&mut state).unwrap();
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Graveyard => state.players[player.index()].graveyard.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}

fn enter(state: &mut GameState, player: PlayerId) -> ObjectId {
    let source = put(state, player, "Vampire Soulcaller", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
    source
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn pass(state: &mut GameState, d: Decision) {
    match d {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        other => panic!("unexpected {other:?}"),
    }
}

fn targets(state: &mut GameState) -> Decision {
    for _ in 0..32 {
        let d = next(state);
        if matches!(d, Decision::ChooseTargets { .. }) {
            return d;
        }
        assert!(!state.stack.is_empty() || !state.engine.pending_triggers.is_empty());
        pass(state, d);
    }
    panic!("no target choice")
}

fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        pass(state, d);
    }
    panic!("did not settle")
}

fn refuse_unchanged(state: &mut GameState, action: Action) {
    let bytes = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), bytes);
}

#[test]
fn soulcaller_printed_characteristics_and_cost_are_exact() {
    let id = card_id_by_name("Vampire Soulcaller").unwrap();
    assert_eq!(id, 343);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.subtypes, &[Subtype::Vampire, Subtype::Warlock]);
    assert_eq!(
        (def.power, def.toughness, def.mana_value),
        (Some(3), Some(2), 5)
    );
    assert_eq!(def.colors, &[ManaColor::B]);
    assert_eq!(def.cost.generic, 4);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::B)]);
    assert!(def.keywords.has(Keywords::FLYING));
    assert_eq!(
        trigger::trigger_target_spec(id),
        TargetSpec::CreatureCardInOwnGraveyard
    );
    let mut state = ready(PlayerId::P0);
    let source = put(&mut state, PlayerId::P0, "Vampire Soulcaller", Zone::Hand);
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[0].mana_pool[5] = 3;
    next(&mut state);
    refuse_unchanged(&mut state, Action::CastSpell(source));
    state.players[0].mana_pool[5] += 1;
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
}

#[test]
fn soulcaller_mandatory_recovery_filters_each_players_graveyard_and_restores() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let wanted = put(&mut state, player, "Faerie Miscreant", Zone::Graveyard);
        let noncreature = put(&mut state, player, "Plains", Zone::Graveyard);
        let theirs = put(
            &mut state,
            player.opponent(),
            "Faerie Miscreant",
            Zone::Graveyard,
        );
        let token = put(&mut state, player, "Cat Token", Zone::Graveyard);
        enter(&mut state, player);
        let d = targets(&mut state);
        assert!(
            matches!(&d, Decision::ChooseTargets { legal_targets, remaining: 1,
            can_finish: false, .. } if legal_targets == &vec![Target::Object(wanted)])
        );
        refuse_unchanged(&mut state, Action::FinishEffectSelection);
        for illegal in [noncreature, theirs, token] {
            refuse_unchanged(&mut state, Action::ChooseTarget(Target::Object(illegal)));
        }
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(next(&mut replay), d);
        for branch in [&mut state, &mut replay] {
            engine::step(branch, Action::ChooseTarget(Target::Object(wanted))).unwrap();
            settle(branch);
            assert_eq!(branch.objects.get(wanted).zone, Zone::Hand);
            assert!(branch.players[player.index()].hand.contains(&wanted));
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn soulcaller_empty_or_noncreature_graveyard_drops_the_targeted_trigger() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        put(&mut state, player, "Plains", Zone::Graveyard);
        put(
            &mut state,
            player.opponent(),
            "Faerie Miscreant",
            Zone::Graveyard,
        );
        enter(&mut state, player);
        settle(&mut state);
        assert!(state.engine.pending_triggers.is_empty());
        assert!(state.stack.is_empty());
    }
}

#[test]
fn soulcaller_recovery_does_not_follow_a_departed_and_returned_card() {
    let mut state = ready(PlayerId::P0);
    let wanted = put(
        &mut state,
        PlayerId::P0,
        "Faerie Miscreant",
        Zone::Graveyard,
    );
    enter(&mut state, PlayerId::P0);
    targets(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(wanted))).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.stack.len(), 1, "trigger has reached the stack");
    event::propose_and_commit(&mut state, ProposedEvent::zone_change(wanted, Zone::Exile));
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(wanted, Zone::Graveyard),
    );
    settle(&mut state);
    assert_eq!(state.objects.get(wanted).zone, Zone::Graveyard);
}

#[test]
fn soulcaller_cannot_block_until_witness_protection_removes_its_printed_ability() {
    let mut state = ready(PlayerId::P0);
    let attacker = put(
        &mut state,
        PlayerId::P0,
        "Treetop Snarespinner",
        Zone::Battlefield,
    );
    let blocker = put(
        &mut state,
        PlayerId::P1,
        "Vampire Soulcaller",
        Zone::Battlefield,
    );
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.attackers_declared = true;
    assert!(
        matches!(next(&mut state), Decision::DeclareBlockers { legal_blockers, .. }
        if legal_blockers.iter().all(|(_, choices)| !choices.contains(&blocker)))
    );
    refuse_unchanged(
        &mut state,
        Action::DeclareBlockers(vec![(blocker, attacker)]),
    );
    state.step = Step::Main1;
    state.engine.combat = Default::default();
    let aura = put(&mut state, PlayerId::P0, "Witness Protection", Zone::Hand);
    state.players[0].mana_pool[ManaColor::U.pool_index()] = 1;
    next(&mut state);
    engine::step(&mut state, Action::CastSpell(aura)).unwrap();
    targets(&mut state);
    engine::step(&mut state, Action::ChooseTarget(Target::Object(blocker))).unwrap();
    settle(&mut state);
    assert!(!engine::has_effective_keyword(
        &state,
        blocker,
        Keywords::FLYING
    ));
    state.step = Step::DeclareBlockers;
    state.engine.combat.attackers = vec![attacker];
    state.engine.combat.attackers_declared = true;
    assert!(
        matches!(next(&mut state), Decision::DeclareBlockers { legal_blockers, .. }
        if legal_blockers.iter().any(|(_, choices)| choices.contains(&blocker)))
    );
    engine::step(
        &mut state,
        Action::DeclareBlockers(vec![(blocker, attacker)]),
    )
    .unwrap();
}
