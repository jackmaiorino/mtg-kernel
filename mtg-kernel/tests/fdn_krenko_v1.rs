//! Genuine future definition only. Every printed case remains ignored until
//! the parent registers and qualifies Krenko; no root/target admission bypass.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardType, CostComponent, Keywords, Subtype, Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItem, Step, Zone};

fn put(state: &mut GameState, actor: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let id = card_id_by_name(name).expect("genuine parent registration prerequisite");
    let object = state.objects.push(GameObject {
        card_def: id,
        name: CARD_DEFS[usize::from(id)].object_name.into(),
        owner: actor,
        controller: actor,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(id),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[actor.index()].hand.push(object),
        Zone::Battlefield => state.players[actor.index()].battlefield.push(object),
        _ => panic!("physical placement zone unsupported"),
    }
    object
}
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        3434,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn next(state: &mut GameState) -> Decision {
    let result = engine::advance_until_decision(state);
    assert!(!matches!(result, Decision::Halted { .. }), "{result:?}");
    result
}
fn pass(state: &mut GameState, decision: Decision) {
    match decision {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::DeclareAttackers { .. } => {
            engine::step(state, Action::DeclareAttackers(vec![])).unwrap()
        }
        other => panic!("unexpected traversal {other:?}"),
    }
}
fn finish(state: &mut GameState) {
    for _ in 0..35 {
        let decision = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        pass(state, decision);
    }
    panic!("resolution did not finish");
}
fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Krenko, Mob Boss", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 4;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn naturally_ready(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    for _ in 0..100 {
        let decision = next(state);
        if state.active_player == actor
            && state.priority_player == actor
            && !state.objects.get(source).summoning_sick
            && matches!(decision, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        pass(state, decision);
    }
    panic!("real Untap did not clear source sickness");
}
fn activate(state: &mut GameState, actor: PlayerId, source: ObjectId) -> StackItem {
    assert_eq!(state.priority_player, actor);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. }
        if activatable_abilities.contains(&(source, 0)))
    );
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    next(state);
    assert!(state.engine.pending_activation.is_none());
    assert!(state.objects.get(source).tapped);
    state
        .stack
        .iter()
        .find(|item| item.source == source)
        .unwrap()
        .clone()
}
fn tokens(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            let live = state.objects.get(id);
            live.v4.is_token
                && live.controller == actor
                && live.zone == Zone::Battlefield
                && live.card_def == card_id_by_name("Goblin Token").unwrap()
        })
        .count()
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    restored
}
fn refuses(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}

#[test]
#[ignore = "Requires genuine parent Krenko registration"]
fn printed_cast_metadata_tap_sickness_natural_untap_and_repeat_resolution_both_seats() {
    let def = &CARD_DEFS[usize::from(card_id_by_name("Krenko, Mob Boss").unwrap())];
    assert_eq!(
        (def.cost.generic, def.cost.x_count, def.mana_value),
        (2, 0, 4)
    );
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)]
    );
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Goblin, Subtype::Warrior]);
    assert_eq!(def.supertypes, &[Supertype::Legendary]);
    assert_eq!((def.power, def.toughness), (Some(3), Some(3)));
    assert_eq!(def.keywords, Keywords::NONE);
    let [ability] = def.activated_abilities else {
        panic!("sole printed tap activation");
    };
    assert_eq!(ability.cost, &[CostComponent::Tap]);
    assert_eq!(ability.target_spec, TargetSpec::None);
    assert!(!ability.sorcery_speed_only && ability.max_activations_per_turn.is_none());
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        refuses(&mut state, Action::ActivateAbility(source, 0));
        naturally_ready(&mut state, actor, source);
        activate(&mut state, actor, source);
        refuses(&mut state, Action::ActivateAbility(source, 0));
        let mut replay = restore(&state);
        finish(&mut state);
        finish(&mut replay);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(tokens(&state, actor), 1);
        // Genuine untap event is a physical accepted primitive, not another
        // turn or an invented once-per-turn exception.
        event::propose_and_commit(&mut state, ProposedEvent::untap(source));
        activate(&mut state, actor, source);
        finish(&mut state);
        assert_eq!(tokens(&state, actor), 3);
    }
}

#[test]
#[ignore = "Requires genuine parent Krenko registration"]
fn real_resolution_samples_current_goblin_permanents_source_departure_and_controller_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for departure in [false, true] {
            let mut state = ready(actor);
            let source = cast(&mut state, actor);
            naturally_ready(&mut state, actor, source);
            let item = activate(&mut state, actor, source);
            // A real token entry after payment must participate in the count.
            let token = card_id_by_name("Goblin Token").unwrap();
            event::propose_and_commit(&mut state, ProposedEvent::create_token(token, actor));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::create_token(token, actor.opponent()),
            );
            let land = put(&mut state, actor, "Mountain", Zone::Battlefield);
            // Explicit physical noncreature subtype boundary.
            state
                .objects
                .get_mut(land)
                .v4
                .effective_subtype_ids
                .push(Subtype::Goblin.stable_id());
            if departure {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
                let old = state.objects.get(source).controller;
                state.players[old.index()]
                    .battlefield
                    .retain(|&id| id != source);
                state.players[actor.opponent().index()]
                    .battlefield
                    .push(source);
                state.objects.get_mut(source).controller = actor.opponent();
            }
            let mut replay = restore(&state);
            finish(&mut state);
            finish(&mut replay);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
            assert_eq!(tokens(&state, actor), if departure { 3 } else { 4 });
            assert_eq!(tokens(&state, actor.opponent()), 1);
            let sample = state
                .engine
                .event_history
                .iter()
                .find_map(|e| match e {
                    CommittedEvent::KrenkoSampleV1 { sample } => Some(sample),
                    _ => None,
                })
                .unwrap();
            assert_eq!(sample.members.len(), if departure { 2 } else { 3 });
            let mut replayed = state.clone();
            replayed.stack.push(item);
            refuses(&mut replayed, Action::Pass);
        }
    }
}

#[test]
#[ignore = "Requires genuine parent Krenko registration"]
fn paid_owner_source_metadata_payment_and_token_occurrence_forgeries_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast(&mut state, actor);
        naturally_ready(&mut state, actor, source);
        activate(&mut state, actor, source);
        for change in 0..5 {
            let mut forged = restore(&state);
            match change {
                0 => forged.stack.clear(),
                1 => forged.stack.last_mut().unwrap().controller = actor.opponent(),
                2 => {
                    forged.stack.last_mut().unwrap().inline_effect =
                        Some(mtg_kernel::effect::EffectOp::Sequence(vec![]))
                }
                3 => {
                    forged
                        .engine
                        .event_history
                        .retain(|e| !matches!(e, CommittedEvent::KrenkoPaidV1 { .. }));
                }
                _ => {
                    let tap = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|e| matches!(e, CommittedEvent::Tap { object } if *object == source))
                        .unwrap();
                    *tap = CommittedEvent::Tap {
                        object: ObjectId(u32::MAX),
                    };
                }
            }
            refuses(&mut forged, Action::Pass);
        }
        finish(&mut state);
        let mut forged = restore(&state);
        let created = forged
            .engine
            .event_history
            .iter_mut()
            .find(|e| matches!(e, CommittedEvent::CreateToken { .. }))
            .unwrap();
        let CommittedEvent::CreateToken { controller, .. } = created else {
            unreachable!()
        };
        *controller = actor.opponent();
        refuses(&mut forged, Action::Pass);
    }
}
