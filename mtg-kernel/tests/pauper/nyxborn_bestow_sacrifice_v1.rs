//! A bestowed Nyxborn Hydra against Fanatical Offering's "sacrifice an
//! artifact or creature" cost. CR 205.1 and 702.103b: while it is attached
//! by bestow the Hydra is an Aura, not a creature, so it is neither offered
//! nor accepted for the cost. Once the enchanted creature leaves, the Hydra
//! is an enchantment creature and is both.

use mtg_kernel::card_def::{card_id_by_name, CardType, CARD_DEFS};
use mtg_kernel::engine::{self, Action, CostKind, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn card_name(card_def: u16) -> String {
    CARD_DEFS[card_def as usize].name.to_string()
}

fn ready_main(seed: u64, active: PlayerId) -> GameState {
    let mut state = GameState::new_from_libraries(&[], &[], card_name, seed);
    state.step = Step::Main1;
    state.active_player = active;
    state.priority_player = active;
    state
}

fn put_object(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id(name);
    let id = state.objects.push(GameObject {
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
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        Zone::Library => state.players[owner.index()].library.push(id),
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Exile => state.exile.push(id),
        Zone::Command => state.command.push(id),
        Zone::Stack => panic!("test helper does not fabricate stack objects"),
    }
    id
}

fn pass_until_idle(state: &mut GameState) {
    for _ in 0..96 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::Halted { mechanic, source } => {
                panic!("unexpected halt {mechanic:?} from {source}")
            }
            other => panic!("unexpected decision while resolving to idle: {other:?}"),
        }
    }
    panic!("bounded resolution did not become idle")
}

/// Casts Nyxborn Hydra for its bestow cost with X = 0 onto `host`.
fn bestow_hydra(state: &mut GameState, me: PlayerId, hydra: ObjectId, host: ObjectId) {
    state.players[me.index()].mana_pool[ManaColor::G.pool_index()] = 2;
    engine::step(state, Action::CastSpell(hydra)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseSpellMode { mode_count: 2, .. }
    ));
    engine::step(state, Action::ChooseSpellMode(1)).unwrap();
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::ChooseTargets { .. }
    ));
    engine::step(state, Action::ChooseTarget(Target::Object(host))).unwrap();
    // With only {G}{G} available X can only be 0; answer the X choice if
    // the engine asks it.
    if let Decision::ChooseEffectOption { .. } = engine::advance_until_decision(state) {
        engine::step(state, Action::ChooseEffectOption(0)).unwrap();
    }
    pass_until_idle(state);
    assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
    assert_eq!(
        state.objects.get(hydra).v4.attached_to.unwrap().object,
        host
    );
    assert!(!engine::object_has_type(state, hydra, CardType::Creature));
}

/// Casts Fanatical Offering and returns the sacrifice candidates offered.
fn cast_offering(state: &mut GameState, me: PlayerId, offering: ObjectId) -> Vec<ObjectId> {
    state.players[me.index()].mana_pool[ManaColor::B.pool_index()] = 1;
    state.players[me.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(state, Action::CastSpell(offering)).unwrap();
    match engine::advance_until_decision(state) {
        Decision::ChooseCostTargets {
            player,
            source,
            cost_kind,
            remaining,
            candidates,
        } => {
            assert_eq!(player, me);
            assert_eq!(source, offering);
            assert_eq!(cost_kind, CostKind::SacrificePermanents);
            assert_eq!(remaining, 1);
            candidates
        }
        other => panic!("expected the sacrifice choice, got {other:?}"),
    }
}

#[test]
fn bestowed_nyxborn_hydra_is_not_a_creature_to_sacrifice_until_unattached() {
    for (me, seed) in [
        (PlayerId::P0, 0x4e59_5853_4143_0001),
        (PlayerId::P1, 0x4e59_5853_4143_0002),
    ] {
        let mut state = ready_main(seed, me);
        for _ in 0..4 {
            put_object(&mut state, me, "Mountain", Zone::Library);
        }
        let host = put_object(&mut state, me, "Voldaren Epicure", Zone::Battlefield);
        let other = put_object(&mut state, me, "Avenging Hunter", Zone::Battlefield);
        let extra = put_object(&mut state, me, "Myr Enforcer", Zone::Battlefield);
        let hydra = put_object(&mut state, me, "Nyxborn Hydra", Zone::Hand);
        let first = put_object(&mut state, me, "Fanatical Offering", Zone::Hand);
        let second = put_object(&mut state, me, "Fanatical Offering", Zone::Hand);
        bestow_hydra(&mut state, me, hydra, host);

        // While bestowed the Hydra is an Aura: not offered, not accepted.
        let candidates = cast_offering(&mut state, me, first);
        assert_eq!(candidates, vec![host, other, extra]);
        assert!(!candidates.contains(&hydra));
        let before = state.clone();
        assert!(engine::step(&mut state, Action::ChooseCostTarget(hydra)).is_err());
        assert_eq!(state, before);
        engine::step(&mut state, Action::ChooseCostTarget(other)).unwrap();
        pass_until_idle(&mut state);
        assert_eq!(state.objects.get(other).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(hydra).v4.attached_to.unwrap().object,
            host
        );

        // The enchanted creature leaves: the Hydra is a creature again.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(host, Zone::Graveyard),
        );
        pass_until_idle(&mut state);
        assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
        assert!(state.objects.get(hydra).v4.attached_to.is_none());
        assert!(engine::object_has_type(&state, hydra, CardType::Creature));

        let candidates = cast_offering(&mut state, me, second);
        assert!(candidates.contains(&hydra));
        assert!(candidates.contains(&extra));
        engine::step(&mut state, Action::ChooseCostTarget(hydra)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(
            state.stack.last().unwrap().v4.paid_cost_refs[0].object,
            hydra
        );
        pass_until_idle(&mut state);
        assert_eq!(state.objects.get(hydra).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(extra).zone, Zone::Battlefield);
    }
}
