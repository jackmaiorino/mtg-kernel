//! Actual printed acceptance requires real registration. No fixture admission.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardType, CostComponent, Subtype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::EffectOp;
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, StackItemKind, Step, Target, Zone};

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
        Zone::Graveyard => state.players[owner.index()].graveyard.push(id),
        Zone::Hand => state.players[owner.index()].hand.push(id),
        _ => panic!("unsupported physical placement"),
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
        11010,
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
            other => panic!("unexpected Ooze resolution choice {other:?}"),
        }
    }
    panic!("Ooze stack did not settle");
}
fn cast_source(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Scavenging Ooze", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state
        .stack
        .iter()
        .any(|item| item.kind == StackItemKind::Spell && item.source == source));
    settle(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    source
}
fn begin(state: &mut GameState, actor: PlayerId, source: ObjectId) -> Vec<Target> {
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, activatable_abilities, .. }
        if player == actor && activatable_abilities.contains(&(source, 0)))
    );
    engine::step(state, Action::ActivateAbility(source, 0)).unwrap();
    match next(state) {
        Decision::ChooseTargets {
            player,
            remaining,
            can_finish,
            legal_targets,
            ..
        } => {
            assert_eq!(player, actor);
            assert_eq!(remaining, 1);
            assert!(!can_finish, "the printed target is mandatory");
            legal_targets
        }
        other => panic!("missing genuine Ooze target choice {other:?}"),
    }
}
fn choose(state: &mut GameState, source: ObjectId, target: ObjectId, actor: PlayerId) {
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let item = state
        .stack
        .iter()
        .rev()
        .find(|item| item.source == source && item.kind == StackItemKind::ActivatedAbility)
        .unwrap();
    assert_eq!(item.controller, actor);
    assert_eq!(item.targets, vec![Target::Object(target)]);
    assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
    assert_eq!(item.v4.target_contracts.len(), 1);
}
fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}
fn change_controller(state: &mut GameState, source: ObjectId, player: PlayerId) {
    // Physical controller primitive, no invented printed control producer.
    let previous = state.objects.get(source).controller;
    state.players[previous.index()]
        .battlefield
        .retain(|id| *id != source);
    state.players[player.index()].battlefield.push(source);
    state.objects.get_mut(source).controller = player;
}
#[test]
fn actual_definition_and_g_activation_have_exact_mandatory_contracts() {
    let id = card_id_by_name("Scavenging Ooze").expect("true printed registration required");
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Ooze]);
    assert_eq!((def.power, def.toughness), (Some(2), Some(2)));
    assert_eq!(def.cost.generic, 1);
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::G)]);
    assert_eq!(def.activated_abilities.len(), 1);
    let ability = def.activated_abilities[0];
    assert_eq!(ability.target_spec, TargetSpec::CardInGraveyards);
    assert!(matches!(ability.cost, [CostComponent::Mana(cost)]
        if cost.generic == 0 && cost.pips == [Pip::Colored(ManaColor::G)] && cost.x_count == 0));
    assert!(!ability.sorcery_speed_only);
    assert_eq!(ability.activation_zone, Zone::Battlefield);
    assert_eq!(
        (ability.effect)(),
        EffectOp::ExileGraveyardTargetThenGrowSourceIfCreature
    );
    assert_eq!(TargetSpec::CardInGraveyards.stable_id(), 65);
    assert_eq!(Subtype::Orc.stable_id(), 146);
    assert_eq!(Subtype::Ooze.stable_id(), 149);
}
#[test]
fn both_seat_actual_activations_exile_either_owners_cards_and_grow_only_for_creature() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for owner in [actor, actor.opponent()] {
            for (name, grows) in [
                ("Llanowar Elves", true),
                ("Lightning Bolt", false),
                ("Forest", false),
            ] {
                let mut state = ready(actor);
                let target = put(&mut state, owner, name, Zone::Graveyard);
                let source = cast_source(&mut state, actor);
                state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
                assert!(begin(&mut state, actor, source).contains(&Target::Object(target)));
                let mut pending = restore(&state);
                for branch in [&mut state, &mut pending] {
                    choose(branch, source, target, actor);
                    assert_eq!(
                        branch.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
                        0
                    );
                }
                let mut announced = restore(&state);
                let before_life = state.players[actor.index()].life;
                for branch in [&mut state, &mut pending, &mut announced] {
                    settle(branch);
                    assert_eq!(branch.objects.get(target).zone, Zone::Exile);
                    assert!(branch.players[owner.index()].exile.contains(&target));
                    assert_eq!(
                        branch.objects.get(source).counters.plus1_plus1,
                        i32::from(grows)
                    );
                    assert_eq!(
                        branch.players[actor.index()].life,
                        before_life + i32::from(grows)
                    );
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&announced).unwrap()
                );
            }
        }
    }
}
#[test]
fn no_graveyard_card_means_no_activation_and_foreign_zone_choices_are_read_only() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. }
            if !activatable_abilities.contains(&(source, 0)))
        );
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::ActivateAbility(source, 0)).is_err());
        assert_eq!(before, serde_json::to_vec(&state).unwrap());
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        let foreign_hand = state.players[actor.opponent().index()].hand[0];
        let menu = begin(&mut state, actor, source);
        assert!(menu.contains(&Target::Object(target)));
        assert!(
            !menu.contains(&Target::Object(source))
                && !menu.contains(&Target::Object(foreign_hand))
        );
        let before = serde_json::to_vec(&state).unwrap();
        for invalid in [source, foreign_hand] {
            assert!(
                engine::step(&mut state, Action::ChooseTarget(Target::Object(invalid))).is_err()
            );
            assert_eq!(before, serde_json::to_vec(&state).unwrap());
        }
        let mut hidden_variant = restore(&state);
        let secret = hidden_variant.players[actor.opponent().index()].hand[0];
        let island = card_id_by_name("Island").unwrap();
        hidden_variant.objects.get_mut(secret).card_def = island;
        hidden_variant.objects.get_mut(secret).name = "Island".into();
        hidden_variant.objects.get_mut(secret).v4 = ObjectStateV4::from_card_def(island);
        assert!(
            matches!(next(&mut hidden_variant), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets == menu)
        );
        choose(&mut state, source, target, actor);
        settle(&mut state);
    }
}
#[test]
fn multiple_actual_activations_same_card_only_first_resolving_activation_has_effect() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 2;
        begin(&mut state, actor, source);
        choose(&mut state, source, target, actor);
        begin(&mut state, actor, source);
        choose(&mut state, source, target, actor);
        assert_eq!(state.stack.len(), 2);
        let before_life = state.players[actor.index()].life;
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            settle(branch);
            assert_eq!(branch.objects.get(target).zone, Zone::Exile);
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 1);
            assert_eq!(branch.players[actor.index()].life, before_life + 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn actual_lightning_bolt_removal_keeps_exile_and_gain_after_ooze_dies() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        begin(&mut state, actor, source);
        choose(&mut state, source, target, actor);
        let bolt = state.players[actor.index()].hand[0];
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(
            matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        let before = state.players[actor.index()].life;
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            settle(branch);
            assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 0);
            assert_eq!(branch.objects.get(target).zone, Zone::Exile);
            assert_eq!(branch.players[actor.index()].life, before + 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
#[test]
fn source_reentry_or_control_change_and_target_reentry_bind_exact_incarnations() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor);
            let source = cast_source(&mut state, actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Graveyard,
            );
            state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
            begin(&mut state, actor, source);
            choose(&mut state, source, target, actor);
            // Physical zone/control primitives; no invented printed response.
            match mode {
                0 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(target, Zone::Graveyard),
                    );
                }
                1 => {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
                2 => change_controller(&mut state, source, actor.opponent()),
                _ => unreachable!(),
            }
            let before = state.players[actor.index()].life;
            let mut restored = restore(&state);
            for branch in [&mut state, &mut restored] {
                settle(branch);
                assert_eq!(
                    branch.objects.get(target).zone,
                    if mode == 0 {
                        Zone::Graveyard
                    } else {
                        Zone::Exile
                    }
                );
                assert_eq!(
                    branch.objects.get(source).counters.plus1_plus1,
                    i32::from(mode == 2)
                );
                assert_eq!(
                    branch.players[actor.index()].life,
                    before + i32::from(mode != 0)
                );
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
#[test]
fn pending_and_announced_definition_forgeries_halt_without_card_or_life_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = cast_source(&mut state, actor);
        let target = put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Graveyard,
        );
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        begin(&mut state, actor, source);
        let pending = restore(&state);
        for mode in 0..3 {
            let mut forged = restore(&pending);
            let activation = forged.engine.pending_activation.as_mut().unwrap();
            match mode {
                0 => activation.target_spec = TargetSpec::UpToOneCardInGraveyards,
                1 => activation.controller = actor.opponent(),
                2 => activation.source_zone_change_count += 1,
                _ => unreachable!(),
            }
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted { .. }
            ));
            assert_eq!(forged.objects.get(target).zone, Zone::Graveyard);
            assert_eq!(forged.objects.get(source).counters.plus1_plus1, 0);
            assert_eq!(
                forged.players[actor.index()].life,
                pending.players[actor.index()].life
            );
            assert_eq!(
                forged.players[actor.index()].mana_pool,
                pending.players[actor.index()].mana_pool
            );
        }
        choose(&mut state, source, target, actor);
        let mut forged = restore(&state);
        forged.stack.last_mut().unwrap().inline_effect = Some(EffectOp::Sequence(vec![]));
        for _ in 0..10 {
            match engine::advance_until_decision(&mut forged) {
                Decision::Halted { .. } => break,
                Decision::CastSpellOrPass { .. } => {
                    engine::step(&mut forged, Action::Pass).unwrap()
                }
                other => panic!("unexpected forged root decision {other:?}"),
            }
        }
        assert!(forged.engine.halted.is_some());
        assert_eq!(forged.objects.get(target).zone, Zone::Graveyard);
        assert_eq!(forged.objects.get(source).counters.plus1_plus1, 0);
    }
}
