//! Hungry Ghoul source preparation. Execute after serial registry admission.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardCapability, CostComponent, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Zone};

fn ready() -> GameState {
    let plains = card_id_by_name("Plains").unwrap();
    let mut state =
        GameState::new_from_libraries(&[plains; 40], &[plains; 40], |_| "Plains".into(), 680);
    state.step = Step::Main1;
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 2;
    state
}

fn put(state: &mut GameState, owner: PlayerId, name: &str) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone: Zone::Battlefield,
        tapped: false,
        summoning_sick: true,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    state.players[owner.index()].battlefield.push(id);
    id
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}

fn settle(state: &mut GameState) {
    for _ in 0..60 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("activation did not settle");
}

fn offered(state: &mut GameState, source: ObjectId) -> bool {
    matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, 0)))
}

fn choice(state: &mut GameState) -> Vec<ObjectId> {
    match next(state) {
        Decision::ChooseCostTargets {
            cost_kind: CostKind::SacrificeCreatures,
            remaining: 1,
            candidates,
            ..
        } => candidates,
        other => panic!("expected sacrifice choice, got {other:?}"),
    }
}

#[test]
fn hungry_ghoul_printed_recipe_is_exact() {
    let id = card_id_by_name("Hungry Ghoul").unwrap();
    assert_eq!(id, 342);
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(
        (def.mana_value, def.power, def.toughness),
        (2, Some(2), Some(2))
    );
    assert_eq!(def.activated_abilities.len(), 1);
    assert_eq!(
        def.activated_abilities[0].cost[1],
        CostComponent::SacrificeOtherControlledCreatures(1)
    );
    assert!(!def.activated_abilities[0].sorcery_speed_only);
}

#[test]
fn hungry_ghoul_requires_another_creature_and_mana() {
    let mut state = ready();
    let source = put(&mut state, PlayerId::P0, "Hungry Ghoul");
    put(&mut state, PlayerId::P1, "Healer's Hawk");
    put(&mut state, PlayerId::P0, "Food Token");
    assert!(!offered(&mut state, source));
    let donor = put(&mut state, PlayerId::P0, "Healer's Hawk");
    state.objects.get_mut(donor).tapped = true;
    state.players[0].mana_pool = [0; 6];
    assert!(!offered(&mut state, source));
    state.players[0].mana_pool[ManaColor::C.pool_index()] = 1;
    assert!(offered(&mut state, source));
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    settle(&mut state);
    assert_eq!(state.objects.get(donor).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
    assert_eq!(state.players[0].mana_pool, [0; 6]);
    assert!(!state.objects.get(source).tapped);
}

#[test]
fn hungry_ghoul_pending_choice_restore_preserves_candidates_and_rejects_self() {
    let mut state = ready();
    let source = put(&mut state, PlayerId::P0, "Hungry Ghoul");
    let first = put(&mut state, PlayerId::P0, "Healer's Hawk");
    let second = put(&mut state, PlayerId::P0, "Cat Token");
    assert!(offered(&mut state, source));
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    assert_eq!(choice(&mut state), vec![first, second]);
    let bytes = serde_json::to_vec(&state).unwrap();
    let snapshot = state.snapshot();
    let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
    restored.restore(&snapshot);
    assert_eq!(state.state_hash(), restored.state_hash());
    let before = restored.clone();
    assert!(engine::step(&mut restored, Action::ChooseCostTarget(source)).is_err());
    assert_eq!(restored, before);
    for game in [&mut state, &mut restored] {
        engine::step(game, Action::ChooseCostTarget(first)).unwrap();
        settle(game);
    }
    assert_eq!(state, restored);
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
    assert_eq!(state.objects.get(second).zone, Zone::Battlefield);
}

#[test]
fn hungry_ghoul_borrowed_creature_goes_to_its_owners_graveyard() {
    let mut state = ready();
    let source = put(&mut state, PlayerId::P0, "Hungry Ghoul");
    let donor = put(&mut state, PlayerId::P1, "Healer's Hawk");
    state.objects.get_mut(donor).controller = PlayerId::P0;
    assert!(offered(&mut state, source));
    engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
    settle(&mut state);
    assert!(state.players[1].graveyard.contains(&donor));
    assert_eq!(state.objects.get(source).counters.plus1_plus1, 1);
}

#[test]
fn hungry_ghoul_old_activation_cannot_counter_a_returned_source() {
    for returns in [false, true] {
        let mut state = ready();
        let source = put(&mut state, PlayerId::P0, "Hungry Ghoul");
        let donor = put(&mut state, PlayerId::P0, "Healer's Hawk");
        assert!(offered(&mut state, source));
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        next(&mut state);
        assert_eq!(state.stack.len(), 1);
        assert_eq!(state.objects.get(donor).zone, Zone::Graveyard);
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        if returns {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
        }
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(state, restored);
        assert_eq!(state.objects.get(source).counters.plus1_plus1, 0);
    }
}
