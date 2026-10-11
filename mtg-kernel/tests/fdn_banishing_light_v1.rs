//! Future printed acceptance requires real Banishing Light registration.
//! No inline roots or test-only definition admission.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{self, EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    FaceDownV1, GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone,
};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("true printed registration required: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
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
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        Zone::Hand => state.players[owner.index()].hand.push(id),
        _ => panic!("fixture placement supports hand or battlefield"),
    }
    id
}
fn ready(actor: PlayerId) -> GameState {
    let cards = ["Forest", "Island", "Mountain", "Swamp", "Lightning Bolt"]
        .map(|name| card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards,
        &cards,
        |id| CARD_DEFS[id as usize].name.into(),
        11007,
        actor,
    );
    state.step = Step::Main1;
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) {
    for _ in 0..40 {
        match next(state) {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected Banishing Light resolution decision {other:?}"),
        }
    }
    panic!("Banishing Light stack did not settle");
}
fn cast_to_trigger(state: &mut GameState, actor: PlayerId) -> (ObjectId, Vec<Target>) {
    let source = put(state, actor, "Banishing Light", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state
        .stack
        .iter()
        .any(|item| item.source == source && item.kind == StackItemKind::Spell));
    for _ in 0..30 {
        match next(state) {
            Decision::ChooseTargets {
                player,
                legal_targets,
                ..
            } if player == actor => return (source, legal_targets),
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty()
                    && state.objects.get(source).zone == Zone::Battlefield =>
            {
                return (source, vec![])
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            other => panic!("unexpected printed ETB target decision {other:?}"),
        }
    }
    panic!("no Banishing Light entry decision");
}
fn announce_target(state: &mut GameState, source: ObjectId, target: ObjectId) {
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let trigger = state
        .stack
        .iter()
        .rev()
        .find(|item| item.source == source && item.kind == StackItemKind::TriggeredAbility)
        .unwrap();
    assert_eq!(trigger.targets, vec![Target::Object(target)]);
    assert_eq!(trigger.v4.target_contracts.len(), 1);
    assert_eq!(
        trigger
            .v4
            .ability_source_contract
            .unwrap()
            .zone_change_count,
        state.objects.get(source).zone_change_count
    );
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn respond_broken_wings(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    let removal = put(state, actor, "Broken Wings", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&removal))
    );
    engine::step(state, Action::CastSpell(removal)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(source)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(source))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == removal));
}
fn change_controller(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    // Physical controller fixture; no invented printed control spell.
    let previous = state.objects.get(object).controller;
    state.players[previous.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
fn grant_keyword_primitive(state: &mut GameState, target: ObjectId, keyword: Keywords) {
    // A real typed duration record, not a forged printed spell root.
    effect::execute(
        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: ObjectRef::ThisSource,
            keyword,
        },
        &ExecCtx::no_targets(target, state.objects.get(target).controller),
        state,
    );
    assert!(engine::has_effective_keyword(state, target, keyword));
}
#[test]
fn printed_definition_has_one_etb_and_opponent_nonland_target_contract() {
    let id =
        card_id_by_name("Banishing Light").expect("true Banishing Light registration required");
    let definition = &CARD_DEFS[id as usize];
    assert_eq!(definition.types, &[CardType::Enchantment]);
    assert_eq!((definition.cost.generic, definition.mana_value), (2, 3));
    assert_eq!(definition.cost.pips, &[Pip::Colored(ManaColor::W)]);
    assert_eq!(definition.target_spec, TargetSpec::None);
    let triggers = mtg_kernel::trigger::triggers_for(id);
    assert_eq!(
        triggers.len(),
        1,
        "return is a duration, not a second printed trigger"
    );
    assert_eq!(
        triggers[0].condition,
        mtg_kernel::trigger::TriggerCondition::Etb
    );
    let root = (triggers[0].effect)();
    assert_eq!(
        root,
        EffectOp::ExileTargetLinkedToSource {
            object: ObjectRef::Target(0)
        }
    );
    assert_eq!(
        mtg_kernel::trigger::target_spec_for_trigger(id, &root),
        Some(TargetSpec::OpponentNonlandPermanent)
    );
}
#[test]
fn actual_both_seat_casts_exile_each_nonland_class_and_restore_exact_return() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for name in ["Llanowar Elves", "Goldvein Pick", "Anthem of Champions"] {
            let mut state = ready(actor);
            let target = put(&mut state, actor.opponent(), name, Zone::Battlefield);
            let own = put(&mut state, actor, name, Zone::Battlefield);
            let land = state.players[actor.opponent().index()].battlefield[0];
            let (source, legal) = cast_to_trigger(&mut state, actor);
            assert!(legal.contains(&Target::Object(target)));
            assert!(!legal.contains(&Target::Object(own)));
            assert!(!legal.contains(&Target::Object(land)));
            assert!(!legal.contains(&Target::Object(source)));
            let mut restored_choice = replay(&state);
            announce_target(&mut state, source, target);
            announce_target(&mut restored_choice, source, target);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored_choice).unwrap()
            );
            let mut announced = replay(&state);
            settle(&mut state);
            settle(&mut announced);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&announced).unwrap()
            );
            assert_eq!(state.objects.get(target).zone, Zone::Exile);
            let record = state.engine.linked_exile_records[0];
            assert_eq!(record.source.source, source);
            assert_eq!(
                record.source.zone_change_count,
                state.objects.get(source).zone_change_count
            );
            assert_eq!(record.exiled, target);
            assert_eq!(
                record.exiled_zone_change_count,
                state.objects.get(target).zone_change_count
            );
            assert_eq!(
                state.objects.get(target).v4.exiled_by.unwrap().object,
                source
            );
            let mut restored = replay(&state);
            for branch in [&mut state, &mut restored] {
                event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
                // Until-leaves expiration commits before another priority/trigger decision.
                assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(target).controller, actor.opponent());
                assert!(branch.objects.get(target).v4.exiled_by.is_none());
                assert!(branch.engine.linked_exile_records.is_empty());
                assert_eq!(branch.objects.get(own).zone, Zone::Battlefield);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
#[test]
fn actual_broken_wings_response_leaves_source_before_trigger_so_nothing_is_exiled() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let (source, _) = cast_to_trigger(&mut state, actor);
        announce_target(&mut state, source, target);
        respond_broken_wings(&mut state, actor, source);
        let mut restored = replay(&state);
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
        assert!(state.engine.linked_exile_records.is_empty());
        assert!(!state.engine.event_history.iter().any(|event| matches!(event,
            event::CommittedEvent::ZoneChange { object, to: Zone::Exile, .. } if *object == target)));
    }
}
#[test]
fn actual_source_removal_returns_exact_exile_to_owner_despite_controller_change() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        change_controller(&mut state, target, actor.opponent());
        let (source, _) = cast_to_trigger(&mut state, actor);
        announce_target(&mut state, source, target);
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Exile);
        change_controller(&mut state, source, actor.opponent());
        // Original owner casts real removal against the now-opposing source.
        respond_broken_wings(&mut state, actor, source);
        let mut restored = replay(&state);
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(target).controller, actor);
        assert!(state.players[actor.index()].battlefield.contains(&target));
        assert!(!state.players[actor.opponent().index()]
            .battlefield
            .contains(&target));
        assert!(state.engine.linked_exile_records.is_empty());
    }
}
#[test]
fn departed_reentered_exile_is_never_returned_by_an_old_duration() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let (source, _) = cast_to_trigger(&mut state, actor);
        announce_target(&mut state, source, target);
        settle(&mut state);
        let original_generation = state.objects.get(target).zone_change_count;
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(target, Zone::Exile));
        assert!(state.objects.get(target).zone_change_count > original_generation);
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(target).zone, Zone::Exile);
            assert!(branch.engine.linked_exile_records.is_empty());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn stale_or_newly_illegal_target_fizzles_without_exile_or_link_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mutation in 0..4 {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let (source, _) = cast_to_trigger(&mut state, actor);
            announce_target(&mut state, source, target);
            match mutation {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Battlefield),
                    );
                }
                1 => change_controller(&mut state, target, actor),
                2 => grant_keyword_primitive(&mut state, target, Keywords::HEXPROOF),
                3 => grant_keyword_primitive(
                    &mut state,
                    target,
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                ),
                _ => unreachable!(),
            }
            let mut restored = replay(&state);
            settle(&mut state);
            settle(&mut restored);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
            assert!(state.engine.linked_exile_records.is_empty());
            assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        }
    }
}
#[test]
fn suppressed_printed_source_abilities_do_not_cancel_an_existing_duration() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let (source, _) = cast_to_trigger(&mut state, actor);
        announce_target(&mut state, source, target);
        settle(&mut state);
        // Typed characteristic fixture, not a claim of a printed producer which
        // turns a plain Enchantment face down. This suppresses its printed
        // abilities while preserving the exact existing battlefield incarnation.
        state.objects.get_mut(source).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: false,
            lookers: 1 << actor.index(),
            hidden_by: None,
            hidden_by_source: None,
        });
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
            assert!(branch.engine.linked_exile_records.is_empty());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn invalid_foreign_or_land_choices_are_read_only_and_no_target_entry_completes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let own = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let land = state.players[actor.opponent().index()].battlefield[0];
        let (source, _) = cast_to_trigger(&mut state, actor);
        let before = serde_json::to_vec(&state).unwrap();
        for candidate in [source, own, land] {
            assert!(
                engine::step(&mut state, Action::ChooseTarget(Target::Object(candidate))).is_err()
            );
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        announce_target(&mut state, source, target);
        settle(&mut state);
        let mut empty = ready(actor);
        let (source, legal) = cast_to_trigger(&mut empty, actor);
        assert!(legal.is_empty());
        assert_eq!(empty.objects.get(source).zone, Zone::Battlefield);
        assert!(empty.engine.linked_exile_records.is_empty());
    }
}

#[test]
fn old_source_incarnation_cannot_exile_after_physical_reentry_new_trigger_is_independent() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let old_target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let new_target = put(
            &mut state,
            actor.opponent(),
            "Goldvein Pick",
            Zone::Battlefield,
        );
        let (source, _) = cast_to_trigger(&mut state, actor);
        announce_target(&mut state, source, old_target);
        let old_generation = state.objects.get(source).zone_change_count;
        // Physical reentry exercises exact source generations. This does not
        // pretend a printed bounce/recast can ignore sorcery timing.
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        assert!(state.objects.get(source).zone_change_count > old_generation);
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(new_target)))
        );
        announce_target(&mut state, source, new_target);
        let generations = state
            .stack
            .iter()
            .filter(|item| item.source == source)
            .map(|item| item.v4.ability_source_contract.unwrap().zone_change_count)
            .collect::<Vec<_>>();
        assert_eq!(generations.len(), 2);
        assert_ne!(generations[0], generations[1]);
        let mut restored = replay(&state);
        settle(&mut state);
        settle(&mut restored);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.objects.get(old_target).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(new_target).zone, Zone::Exile);
        assert_eq!(state.engine.linked_exile_records.len(), 1);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(new_target).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(old_target).zone, Zone::Battlefield);
            assert!(branch.engine.linked_exile_records.is_empty());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}

#[test]
fn transferred_source_can_target_itself_and_returns_immediately_as_a_new_entry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        let (source, _) = cast_to_trigger(&mut state, actor);
        let original_generation = state.objects.get(source).zone_change_count;
        // Actual physical controller substrate before the existing mandatory
        // placement choice, not an invented printed control spell.
        change_controller(&mut state, source, actor.opponent());
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { player, legal_targets, .. }
            if player==actor && legal_targets.contains(&Target::Object(source)))
        );
        announce_target(&mut state, source, source);
        for _ in 0..30 {
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::ChooseTargets { player, .. } => {
                    assert_eq!(player, actor);
                    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
                    assert_eq!(state.objects.get(source).controller, actor);
                    assert_eq!(
                        state.objects.get(source).zone_change_count,
                        original_generation + 2
                    );
                    assert!(state.engine.linked_exile_records.is_empty());
                    let mut restored = replay(&state);
                    assert_eq!(next(&mut state), next(&mut restored));
                    break;
                }
                other => panic!("unexpected self-exile return checkpoint: {other:?}"),
            }
        }
        assert_eq!(
            state.objects.get(source).zone_change_count,
            original_generation + 2
        );
    }
}

#[test]
fn known_partial_planeswalker_is_a_nonland_recipient_primitive_not_admitted_ajani_gameplay() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Ajani, Caller of the Pride",
            Zone::Hand,
        );
        let definition = &CARD_DEFS[state.objects.get(target).card_def as usize];
        assert!(
            !definition.has_full_support(),
            "this fixture must not invent Full Ajani admission"
        );
        let starting = u32::from(definition.starting_loyalty.unwrap());
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        assert_eq!(
            mtg_kernel::planeswalker_v1::loyalty(&state, target),
            Some(starting)
        );
        let (source, legal) = cast_to_trigger(&mut state, actor);
        assert!(legal.contains(&Target::Object(target)));
        announce_target(&mut state, source, target);
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Exile);
        assert_eq!(mtg_kernel::planeswalker_v1::loyalty(&state, target), None);
        let mut restored = replay(&state);
        for branch in [&mut state, &mut restored] {
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
            assert_eq!(
                mtg_kernel::planeswalker_v1::loyalty(branch, target),
                Some(starting)
            );
            assert!(branch.engine.linked_exile_records.is_empty());
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
