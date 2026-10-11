//! Future printed acceptance requires actual serial FDN registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{
    card_id_by_name, CardCapability, CostComponent, Keywords, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectOp, ExecCtx, ObjectRef};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{
    CastMethodV4, GameObject, GameState, ObjectStateV4, StackTargetContractV4, Step, Target, Zone,
};
fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_320,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).expect("actual registered definition required");
    let id = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
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
    state.players[owner.index()].hand.push(id);
    if zone != Zone::Hand {
        event::propose_and_commit(state, ProposedEvent::zone_change(id, zone));
    }
    id
}
fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
fn settle(state: &mut GameState) {
    for _ in 0..50 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("did not settle");
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn readonly(state: &mut GameState, action: Action) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(engine::step(state, action).is_err());
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
}
fn begin(state: &mut GameState, actor: PlayerId, flashback: bool) -> ObjectId {
    let source = put(
        state,
        actor,
        "Self-Reflection",
        if flashback {
            Zone::Graveyard
        } else {
            Zone::Hand
        },
    );
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] =
        if flashback { 4 } else { 6 };
    next(state);
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(
        matches!(next(state),Decision::ChooseTargets {spell,remaining:1,can_finish:false,..} if spell==source)
    );
    source
}
fn finalize(state: &mut GameState, target: ObjectId) {
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
}
#[test]
fn actual_copy_of_hidden_creature_uses_neutral_pre_entry_definition_and_public_name_both_seats() {
    let neutral = card_id_by_name("Nameless Creature Copy Token")
        .expect("future internal blueprint admission with actual Self-Reflection");
    let def = &CARD_DEFS[neutral as usize];
    assert!(def.object_name.is_empty());
    assert_eq!(def.types, [mtg_kernel::card_def::CardType::Creature]);
    assert!(def.subtypes.is_empty() && def.supertypes.is_empty() && def.colors.is_empty());
    assert!(def.cost.pips.is_empty());
    assert_eq!(
        (def.cost.generic, def.cost.x_count, def.mana_value),
        (0, 0, 0)
    );
    assert_eq!((def.power, def.toughness), (Some(2), Some(2)));
    assert!(def.keywords == Keywords::NONE && def.activated_abilities.is_empty());
    assert!(mtg_kernel::trigger::triggers_for(neutral).is_empty());
    for actor in [PlayerId::P0, PlayerId::P1] {
        for hidden_name in ["Healer of the Glade", "Llanowar Elves"] {
            let mut state = ready(actor);
            let target = put(&mut state, actor, hidden_name, Zone::Hand);
            // Accepted hidden-object primitive stand-in: FDN has no admitted
            // printed face-down producer. Set characteristics before real entry
            // observation and copy casting; do not claim the producer's text.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(target, Zone::Battlefield),
            );
            state.objects.get_mut(target).v4.face_down_v1 = Some(mtg_kernel::state::FaceDownV1 {
                disguised: true,
                lookers: 1 << actor.index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            settle(&mut state);
            let life_before = [state.players[0].life, state.players[1].life];
            let source = begin(&mut state, actor, false);
            finalize(&mut state, target);
            let start = state.engine.event_history.len();
            let mut restored = restore(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                let entries: Vec<_> = current.engine.event_history[start..]
                    .iter()
                    .filter_map(|event| {
                        if let mtg_kernel::event::CommittedEvent::CreateToken {
                            object,
                            token_def,
                            controller,
                        } = event
                        {
                            Some((*object, *token_def, *controller))
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(entries.len(), 1);
                let (token, entry_def, controller) = entries[0];
                assert_eq!(
                    (entry_def, controller),
                    (neutral, actor),
                    "actual ETB sees neutral definition"
                );
                let live = current.objects.get(token);
                assert_eq!(live.card_def, neutral);
                assert_ne!(live.card_def, current.objects.get(target).card_def);
                assert!(live.name.is_empty() && live.v4.face_down_v1.is_none());
                assert!(live.v4.is_token && live.summoning_sick);
                assert_eq!((live.owner, live.controller), (actor, actor));
                assert_eq!(live.v4.ward_generic, 0);
                assert_eq!(engine::effective_name(current, token), "");
                assert_eq!(engine::effective_power(current, token), 2);
                assert_eq!(engine::effective_toughness(current, token), 2);
                assert_eq!(
                    [current.players[0].life, current.players[1].life],
                    life_before,
                    "hidden printed Healer ETB must not be copied"
                );
                assert_eq!(current.objects.get(source).zone, Zone::Graveyard);
                for viewer in [actor, actor.opponent()] {
                    let observation = mtg_kernel::rl::observe_v1(current, viewer, 0).unwrap();
                    let public = observation.projection.battlefield[actor.index()]
                        .iter()
                        .find(|card| card.stable.arena_id == token.0)
                        .unwrap();
                    assert_eq!(public.stable.card_db_id, neutral);
                    assert!(public.card_name.is_empty());
                    let dto = serde_json::to_string(public).unwrap();
                    assert!(
                        !dto.contains(hidden_name) && !dto.contains("Nameless Creature Copy Token")
                    );
                }
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
fn transfer(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let former = state.objects.get(object).controller;
    state.players[former.index()]
        .battlefield
        .retain(|id| *id != object);
    state.players[controller.index()].battlefield.push(object);
    state.objects.get_mut(object).controller = controller;
}
#[test]
fn printed_definition_real_normal_and_flashback_cast_stolen_owner_copy_etb_and_restore_both_seats()
{
    let def =
        &CARD_DEFS[card_id_by_name("Self-Reflection").expect("future FDN registration") as usize];
    assert_eq!(def.capability, CardCapability::Full);
    assert_eq!(def.cost.generic, 4);
    assert_eq!(
        def.cost.pips,
        &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)]
    );
    assert_eq!(def.target_spec, TargetSpec::ControlledCreature);
    assert_eq!(
        (def.spell_effect)(),
        Some(EffectOp::CreateControlledCreatureTargetCopyV1)
    );
    assert!(
        matches!(def.flashback.unwrap().cost,[CostComponent::Mana(cost)] if cost.generic==3 && cost.pips==[Pip::Colored(ManaColor::U)])
    );
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor);
            let target = put(
                &mut state,
                actor.opponent(),
                "Healer of the Glade",
                Zone::Battlefield,
            );
            transfer(&mut state, target, actor);
            settle(&mut state);
            let before_life = state.players[actor.index()].life;
            let source = begin(&mut state, actor, flashback);
            let saved = restore(&state);
            let pending = state.engine.pending_cast.as_ref().unwrap();
            assert_eq!(pending.is_flashback, flashback);
            let opponent = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            readonly(&mut state, Action::ChooseTarget(Target::Object(opponent)));
            state = saved;
            finalize(&mut state, target);
            let item = state.stack.last().unwrap();
            assert_eq!(
                item.v4.cast_method,
                Some(if flashback {
                    CastMethodV4::Flashback
                } else {
                    CastMethodV4::Normal
                })
            );
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::U.pool_index()],
                0
            );
            let mut restored = restore(&state);
            for current in [&mut state, &mut restored] {
                settle(current);
                assert_eq!(
                    current.objects.get(source).zone,
                    if flashback {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
                let tokens = current.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter_map(|id| {
                        let card = current.objects.get(*id);
                        (card.v4.is_token && card.card_def == current.objects.get(target).card_def)
                            .then_some(*id)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(tokens.len(), 1);
                let token = current.objects.get(tokens[0]);
                assert_eq!((token.owner, token.controller), (actor, actor));
                assert_eq!(current.objects.get(target).owner, actor.opponent());
                assert_eq!(
                    current.players[actor.index()].life,
                    before_life + 3,
                    "actual copied Healer ETB"
                );
                assert!(token.summoning_sick);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
#[test]
fn actual_response_target_departure_reentry_control_protection_and_flashback_fizzle_create_no_token(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            for response in 0..4 {
                let mut state = ready(actor);
                let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                settle(&mut state);
                let source = begin(&mut state, actor, flashback);
                finalize(&mut state, target);
                let opponent = actor.opponent();
                match response {
                    0 => {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Hand),
                        );
                    }
                    1 => {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Hand),
                        );
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(target, Zone::Battlefield),
                        );
                    }
                    2 => transfer(&mut state, target, opponent),
                    _ => {
                        let mut ctx = ExecCtx::no_targets(source, actor);
                        ctx.targets = vec![Target::Object(target)];
                        ctx.target_contracts =
                            vec![StackTargetContractV4::capture(&state, ctx.targets[0])];
                        mtg_kernel::effect::execute(
                            &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                                object: ObjectRef::Target(0),
                                keyword: Keywords::PROTECTION_FROM_MONOCOLORED,
                            },
                            &ctx,
                            &mut state,
                        );
                    }
                }
                state = restore(&state);
                settle(&mut state);
                assert_eq!(
                    state.objects.get(source).zone,
                    if flashback {
                        Zone::Exile
                    } else {
                        Zone::Graveyard
                    }
                );
                assert!(!state.players[actor.index()]
                    .battlefield
                    .iter()
                    .any(|id| state.objects.get(*id).v4.is_token));
            }
        }
    }
}
#[test]
fn normal_then_actual_flashback_reuses_physical_spell_and_token_target_with_new_stack_authority() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        settle(&mut state);
        let source = begin(&mut state, actor, false);
        finalize(&mut state, target);
        let first_id = state.stack.last().unwrap().v4.stack_item_id;
        settle(&mut state);
        let token = *state.players[actor.index()].battlefield.last().unwrap();
        assert!(state.objects.get(token).v4.is_token);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 4;
        next(&mut state);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        next(&mut state);
        state = restore(&state);
        finalize(&mut state, token);
        assert_ne!(state.stack.last().unwrap().v4.stack_item_id, first_id);
        assert!(state.stack.last().unwrap().is_flashback);
        settle(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Exile);
        assert_eq!(
            state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|id| state.objects.get(**id).v4.is_token)
                .count(),
            2
        );
    }
}
#[test]
fn no_controlled_target_short_payment_and_coherent_source_method_forgery_refuse_read_only() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut empty = ready(actor);
        let source = put(&mut empty, actor, "Self-Reflection", Zone::Hand);
        empty.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 6;
        next(&mut empty);
        readonly(&mut empty, Action::CastSpell(source));
        let target = put(&mut empty, actor, "Llanowar Elves", Zone::Battlefield);
        settle(&mut empty);
        empty.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 5;
        readonly(&mut empty, Action::CastSpell(source));
        empty.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 6;
        engine::step(&mut empty, Action::CastSpell(source)).unwrap();
        next(&mut empty);
        finalize(&mut empty, target);
        for corrupt in 0..3 {
            let mut forged = restore(&empty);
            let item = forged.stack.last_mut().unwrap();
            match corrupt {
                0 => {
                    item.is_flashback = true;
                    item.v4.cast_method = Some(CastMethodV4::Flashback);
                    item.v4.source_contract.as_mut().unwrap().cast_method = CastMethodV4::Flashback;
                }
                1 => item.v4.target_spec = Some(TargetSpec::Creature),
                _ => {
                    if let StackTargetContractV4::Object {
                        zone_change_count, ..
                    } = &mut item.v4.target_contracts[0]
                    {
                        *zone_change_count += 1;
                    }
                }
            }
            readonly(&mut forged, Action::Pass);
        }
    }
}
