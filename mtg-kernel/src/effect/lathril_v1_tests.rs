//! FDN242 native Partial. Paid printing/actions; explicit native primitives
//! exercise replacement, suppression, control and storage boundaries separately.
use super::*;
use crate::{
    card_def::{self, CardCapability, Keywords, Subtype},
    engine::{self, Action, Decision},
    event::{ActiveReplacement, CommittedEvent, ProposedEvent, ReplacementEffectKind},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::{advance_to_next_main_for, put},
    state::{GameState, StackItemKind, Step, Zone},
};
const NAME: &str = "Lathril, Blade of the Elves";
fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
fn ready(actor: PlayerId) -> GameState {
    let library = [id("Forest"); 40];
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &library,
        &library,
        crate::rl::card_name,
        110_468,
        actor,
    );
    state.step = Step::Main1;
    for _ in 0..10 {
        for name in ["Forest", "Swamp", "Island"] {
            put(&mut state, actor, name, Zone::Battlefield);
        }
    }
    state
}
fn respond(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        // Native legend choice for a genuinely paid Self-Reflection copy.
        Decision::ChooseLegendPermanent { candidates, .. } => {
            Action::ChooseLegendPermanent(*candidates.last().unwrap())
        }
        other => panic!("Lathril native decision {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
}
fn finish(state: &mut GameState) {
    for _ in 0..64 {
        let decision = engine::advance_until_decision(state);
        assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        respond(state, decision);
    }
    panic!("Lathril native resolution did not finish");
}
fn taps(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&object| {
            state.objects.get(object).tapped
                && card_def::CARD_DEFS[usize::from(state.objects.get(object).card_def)].is_land
        })
        .count()
}
fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<Target>,
    cost: usize,
) -> ObjectId {
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    let source = put(state, actor, name, Zone::Hand);
    let before = taps(state, actor);
    engine::step(state, Action::CastSpell(source)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(engine::advance_until_decision(state), Decision::ChooseTargets { ref legal_targets, .. } if legal_targets.contains(&target))
        );
        engine::step(state, Action::ChooseTarget(target)).unwrap();
    }
    assert!(
        matches!(engine::advance_until_decision(state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    assert!(state.engine.pending_cast.is_none());
    let row = state
        .stack
        .iter()
        .find(|row| row.source == source && row.kind == StackItemKind::Spell)
        .unwrap();
    assert_eq!(row.controller, actor);
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    assert_eq!(taps(state, actor) - before, cost);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    source
}
fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready(actor);
    let source = cast(&mut state, actor, NAME, None, 4);
    finish(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    (state, source)
}
fn restore(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&restored, state);
    assert_eq!(restored.state_hash(), state.state_hash());
    restored
}
fn tokens(state: &GameState, actor: PlayerId) -> usize {
    state
        .objects
        .iter()
        .filter(|(_, o)| {
            o.zone == Zone::Battlefield
                && o.controller == actor
                && o.card_def == id("Elf Warrior Token")
        })
        .count()
}
fn create_elves(state: &mut GameState, actor: PlayerId) -> Vec<ObjectId> {
    let start = state.engine.event_history.len();
    event::propose_and_commit_batch(
        state,
        (0..11)
            .map(|_| ProposedEvent::create_token(id("Elf Warrior Token"), actor))
            .collect(),
    );
    let created = state.engine.event_history[start..]
        .iter()
        .filter_map(|entry| match entry {
            CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
                ..
            } if *token_def == id("Elf Warrior Token") && *controller == actor => Some(*object),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(created.len(), 11);
    created
}

fn publish_from_primitive(state: &mut GameState, source: ObjectId, player: PlayerId, amount: i32) {
    // Explicit native combat packet primitive. No attack/power claim for this amount.
    engine::commit_combat_damage_events(
        state,
        vec![ProposedEvent::damage(
            source,
            Target::Player(player),
            amount,
        )],
    );
    assert!(matches!(
        engine::advance_until_decision(state),
        Decision::CastSpellOrPass { .. }
    ));
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
}

#[test]
fn exact_partial_printing_cost_trigger_and_opaque_audit() {
    let card = &card_def::CARD_DEFS[usize::from(id(NAME))];
    assert_eq!(id(NAME), 468);
    assert_eq!(card.capability, CardCapability::Partial);
    assert_eq!(
        (card.mana_value, card.power, card.toughness),
        (4, Some(2), Some(3))
    );
    assert!(card.keywords.contains(Keywords::MENACE));
    assert!(card.subtypes.contains(&Subtype::Elf) && card.subtypes.contains(&Subtype::Noble));
    assert_eq!(card.activated_abilities.len(), 1);
    assert_eq!(
        card.activated_abilities[0].cost,
        &[
            card_def::CostComponent::Tap,
            card_def::CostComponent::TapControlled {
                count: 10,
                filter: card_def::PermanentFilter::AnotherWithSubtype(Subtype::Elf)
            },
        ]
    );
    let [trigger] = crate::trigger::triggers_for(id(NAME)) else {
        panic!("one printed trigger");
    };
    assert_eq!(
        trigger.condition,
        crate::trigger::TriggerCondition::DealsCombatDamageToPlayer
    );
    assert_eq!((trigger.effect)(), EffectOp::BindLathrilCombatTokensV1);
    assert_eq!(
        crate::trigger::target_spec_for_trigger(id(NAME), &(trigger.effect)()),
        Some(card_def::TargetSpec::None)
    );
    assert!(!crate::trigger::trigger_effect_matches(
        id(NAME),
        &EffectOp::CreateTokensDynamic {
            token_def: id("Elf Warrior Token"),
            controller: PlayerRef::Controller,
            count: card_def::DynamicValueDef::Fixed(2),
            tapped: false,
        }
    ));
}

#[test]
fn paid_ten_other_elf_cost_selection_restore_and_readonly_forgeries_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        // Explicit native token-entry setup, not a paid Elf-producing spell.
        // Both sides have eleven otherwise valid untapped Elves. Only the
        // source's actual tap-symbol sickness prevents the first activation.
        let mut sick_source = state.clone();
        let sick_elves = create_elves(&mut sick_source, actor);
        assert!(sick_elves
            .iter()
            .all(|&elf| sick_source.objects.get(elf).summoning_sick));
        assert!(engine::step(&mut sick_source, Action::ActivateAbility(source, 0)).is_err());
        advance_to_next_main_for(&mut state, actor);
        assert!(!state.objects.get(source).summoning_sick);
        let elves = create_elves(&mut state, actor);
        assert!(elves
            .iter()
            .all(|&elf| state.objects.get(elf).summoning_sick));
        let other = put(&mut state, actor, "Bear Cub", Zone::Battlefield);
        let mut tapped_source = state.clone();
        tapped_source.objects.get_mut(source).tapped = true; // explicit cost permission primitive
        assert!(engine::step(&mut tapped_source, Action::ActivateAbility(source, 0)).is_err());
        let mut nine = state.clone();
        nine.objects.get_mut(elves[9]).tapped = true;
        nine.objects.get_mut(elves[10]).tapped = true;
        assert!(
            engine::step(&mut nine, Action::ActivateAbility(source, 0)).is_err(),
            "source plus nine cannot pay"
        );
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        let Decision::ChooseCostTargets {
            cost_kind,
            remaining,
            candidates,
            ..
        } = engine::advance_until_decision(&mut state)
        else {
            panic!("real ten-Elf choice");
        };
        assert_eq!(cost_kind, engine::CostKind::TapPermanents);
        assert_eq!(remaining, 10);
        assert_eq!(candidates, elves);
        assert!(!candidates.contains(&source) && !candidates.contains(&other));
        let unchanged = state.clone();
        assert!(engine::step(&mut state, Action::ChooseCostTarget(source)).is_err());
        assert_eq!(state, unchanged);
        engine::step(&mut state, Action::ChooseCostTarget(elves[0])).unwrap();
        engine::advance_until_decision(&mut state);
        engine::validate_pending_activation(
            &state,
            state.engine.pending_activation.as_ref().unwrap(),
        )
        .unwrap();
        for mode in 0..6 {
            let mut forged = state.clone();
            match mode {
                0 => {
                    forged
                        .engine
                        .pending_activation
                        .as_mut()
                        .unwrap()
                        .object_cost_chosen[0]
                        .object = source
                }
                1 => forged.objects.get_mut(elves[0]).tapped = true,
                2 => forged.objects.get_mut(elves[0]).controller = actor.opponent(),
                3 => forged.objects.get_mut(elves[0]).zone_change_count += 1,
                5 => {
                    forged
                        .engine
                        .pending_activation
                        .as_mut()
                        .unwrap()
                        .object_cost_chosen[0]
                        .object = other
                }
                _ => {
                    let first = forged
                        .engine
                        .pending_activation
                        .as_ref()
                        .unwrap()
                        .object_cost_chosen[0];
                    forged
                        .engine
                        .pending_activation
                        .as_mut()
                        .unwrap()
                        .object_cost_chosen
                        .push(first);
                }
            }
            let before = forged.clone();
            assert!(engine::validate_pending_activation(
                &forged,
                forged.engine.pending_activation.as_ref().unwrap()
            )
            .is_err());
            assert_eq!(forged, before);
        }
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            for &elf in &elves[1..10] {
                assert!(
                    matches!(engine::advance_until_decision(branch), Decision::ChooseCostTargets { ref candidates, .. } if candidates.contains(&elf))
                );
                engine::step(branch, Action::ChooseCostTarget(elf)).unwrap();
            }
            assert!(
                matches!(engine::advance_until_decision(branch), Decision::CastSpellOrPass { player, .. } if player == actor)
            );
            assert!(branch.engine.pending_activation.is_none());
            let row = branch.stack.last().unwrap();
            assert_eq!(
                (
                    row.source,
                    row.controller,
                    row.kind,
                    row.v4.activated_ability_index
                ),
                (source, actor, StackItemKind::ActivatedAbility, Some(0))
            );
            assert!(branch.objects.get(source).tapped);
            assert!(elves[..10]
                .iter()
                .all(|&elf| branch.objects.get(elf).tapped));
            assert!(!branch.objects.get(elves[10]).tapped);
            let life = branch.players.each_ref().map(|p| p.life);
            let activation_id = row.v4.stack_item_id;
            // Explicit native ability-counter primitive, no paid Stifle claim.
            let mut countered = branch.clone();
            assert!(
                engine::counter_stack_item_by_id(&mut countered, activation_id)
                    .unwrap()
                    .is_some()
            );
            finish(&mut countered);
            assert_eq!(countered.players.each_ref().map(|p| p.life), life);
            assert!(countered.objects.get(source).tapped);
            assert!(elves[..10]
                .iter()
                .all(|&elf| countered.objects.get(elf).tapped));
            assert!(!countered.objects.get(elves[10]).tapped);
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            finish(branch);
            assert_eq!(branch.players[actor.index()].life, life[actor.index()] + 10);
            assert_eq!(
                branch.players[actor.opponent().index()].life,
                life[actor.opponent().index()] - 10
            );
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn paid_combat_capture_publication_restore_departure_and_single_consumption_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        advance_to_next_main_for(&mut state, actor);
        crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
        let mut attacked = false;
        for _ in 0..48 {
            let decision = engine::advance_until_decision(&mut state);
            if state.stack.last().is_some_and(|row| {
                matches!(
                    row.inline_effect,
                    Some(EffectOp::LathrilCombatTokensV1 { .. })
                )
            }) {
                break;
            }
            match decision {
                Decision::DeclareAttackers { .. } => {
                    engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
                    attacked = true;
                }
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                }
                other => respond(&mut state, other),
            }
        }
        assert!(attacked);
        let row = state.stack.last().unwrap().clone();
        let capture = event::validate_lathril_trigger_v1(&state, &row).unwrap();
        event::validate_published_lathril_v1(&state, &row).unwrap();
        assert_eq!(event::lathril_damage_v1(&state, capture).unwrap().1, 2);
        assert_eq!(state.players[actor.opponent().index()].life, 18);
        assert_eq!(tokens(&state, actor), 0);
        let mut restored = restore(&state);
        for branch in [&mut state, &mut restored] {
            // Genuine zone-change primitive after actual combat. The independent
            // trigger survives departure and a new incarnation/controller.
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                branch,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            // Explicit nonpaid control primitive on the reentered incarnation.
            branch.players[actor.index()]
                .battlefield
                .retain(|&id| id != source);
            branch.players[actor.opponent().index()]
                .battlefield
                .push(source);
            branch.objects.get_mut(source).controller = actor.opponent();
            finish(branch);
            assert_eq!(tokens(branch, actor), 2);
            assert_eq!(tokens(branch, actor.opponent()), 0);
            assert!(event::validate_published_lathril_v1(branch, &row).is_err());
            assert_eq!(branch.engine.event_history.iter().filter(|entry| matches!(entry,
                CommittedEvent::LathrilCombatResolvedV1 { capture_history_index } if *capture_history_index == capture)).count(), 1);
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn paid_capture_direct_api_and_readonly_history_row_forgeries_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        advance_to_next_main_for(&mut state, actor);
        publish_from_primitive(&mut state, source, actor.opponent(), 3);
        let row = state.stack.last().unwrap().clone();
        let capture = event::validate_lathril_trigger_v1(&state, &row).unwrap();
        let before = state.clone();
        // Explicit native counter primitive; no claim of a printed Stifle route.
        let mut countered = state.clone();
        assert!(
            engine::counter_stack_item_by_id(&mut countered, row.v4.stack_item_id)
                .unwrap()
                .is_some()
        );
        assert_eq!(tokens(&countered, actor), 0);
        countered.stack.push(row.clone());
        assert!(
            engine::validated_stack_item_target_spec(&row, &countered).is_err(),
            "countered occurrence cannot be replayed"
        );
        let mut ctx = crystal_prevention_v1::context(&row);
        ctx.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(&state, &row);
        for op in [
            EffectOp::CreateTokensDynamic {
                token_def: id("Elf Warrior Token"),
                controller: PlayerRef::Controller,
                count: card_def::DynamicValueDef::Fixed(3),
                tapped: false,
            },
            row.inline_effect.clone().unwrap(),
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ] {
            let mut direct = state.clone();
            assert!(begin_resumable_resolution(&op, &ctx, row.clone(), &mut direct).is_err());
            assert_eq!(direct, state);
            execute(&op, &ctx, &mut direct);
            assert!(direct.engine.halted.is_some());
            direct.engine.halted = None;
            assert_eq!(direct, state);
            // Changed root/kind and erased continuation cannot select a public driver.
            let mut changed = row.clone();
            changed.inline_effect = Some(op);
            changed.kind = StackItemKind::ActivatedAbility;
            direct.engine.pending_effect = Some(EffectContinuation {
                resolving_item: changed,
                ctx: ctx.clone(),
                frames: vec![],
                choice: None,
                answered_choice_guard: None,
            });
            let unchanged = direct.clone();
            assert!(validate_pending_effect_choice(&direct).is_err());
            assert!(resume_resumable_resolution(&mut direct).is_err());
            assert_eq!(direct, unchanged);
        }
        // Retain the genuine supplied/public item stamp and publication while
        // erasing the independent context stamp and changing root, kind and stage.
        // This formerly selected the public Draw driver instead of refusing.
        let draw = EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 1,
        };
        let mut erased_ctx = ctx.clone();
        erased_ctx.stack_item_id = None;
        let mut changed_item = row.clone();
        changed_item.kind = StackItemKind::Spell;
        changed_item.inline_effect = Some(draw.clone());
        let mut direct = state.clone();
        assert!(
            begin_resumable_resolution(&draw, &erased_ctx, changed_item.clone(), &mut direct)
                .is_err()
        );
        assert_eq!(direct, state);
        execute(&draw, &erased_ctx, &mut direct);
        assert!(direct.engine.halted.is_some());
        direct.engine.halted = None;
        assert_eq!(direct, state);
        direct.engine.pending_effect = Some(EffectContinuation {
            resolving_item: changed_item,
            ctx: erased_ctx.clone(),
            frames: vec![EffectFrame::Program {
                op: draw.clone(),
                path: vec![],
            }],
            choice: None,
            answered_choice_guard: None,
        });
        let unchanged = direct.clone();
        assert!(validate_pending_effect_choice(&direct).is_err());
        assert!(resume_resumable_resolution(&mut direct).is_err());
        assert_eq!(direct, unchanged);
        execute(&draw, &erased_ctx, &mut direct);
        assert!(direct.engine.halted.is_some());
        direct.engine.halted = None;
        assert_eq!(direct, unchanged);

        // A different genuine activation by that same source remains ordinary,
        // even while this combat trigger and its original public row wait below it.
        let mut activated = state.clone();
        let elves = create_elves(&mut activated, actor); // explicit native token setup
        engine::step(&mut activated, Action::ActivateAbility(source, 0)).unwrap();
        for &elf in &elves[..10] {
            assert!(
                matches!(engine::advance_until_decision(&mut activated), Decision::ChooseCostTargets { ref candidates, .. } if candidates.contains(&elf))
            );
            engine::step(&mut activated, Action::ChooseCostTarget(elf)).unwrap();
        }
        engine::advance_until_decision(&mut activated);
        assert_eq!(
            activated.stack.last().unwrap().kind,
            StackItemKind::ActivatedAbility
        );
        assert_ne!(
            activated.stack.last().unwrap().v4.stack_item_id,
            row.v4.stack_item_id
        );
        assert!(activated.stack.iter().any(|item| item == &row));
        let life = activated.players.each_ref().map(|player| player.life);
        finish(&mut activated);
        assert_eq!(
            activated.players[actor.index()].life,
            life[actor.index()] + 10
        );
        assert_eq!(
            activated.players[actor.opponent().index()].life,
            life[actor.opponent().index()] - 10
        );
        assert_eq!(tokens(&activated, actor), 14);
        assert!(activated.objects.get(source).tapped);
        assert!(elves[..10]
            .iter()
            .all(|&elf| activated.objects.get(elf).tapped));

        for mode in 0..10 {
            let mut forged = state.clone();
            match mode {
                0 => forged.stack.last_mut().unwrap().controller = actor.opponent(),
                1 => {
                    forged.stack.last_mut().unwrap().inline_effect =
                        Some(EffectOp::LathrilCombatTokensV1 {
                            capture_history_index: capture + 1,
                        })
                }
                2 => {
                    if let CommittedEvent::LathrilCombatDamageV1 { amount, .. } =
                        &mut forged.engine.event_history[capture as usize]
                    {
                        *amount += 1;
                    }
                }
                3 => {
                    let CommittedEvent::LathrilCombatDamageV1 {
                        damage_history_index,
                        ..
                    } = forged.engine.event_history[capture as usize]
                    else {
                        unreachable!()
                    };
                    if let CommittedEvent::Damage { amount, .. } =
                        &mut forged.engine.event_history[damage_history_index as usize]
                    {
                        *amount += 1;
                    }
                }
                4 => forged.engine.event_history.retain(|entry| {
                    !matches!(entry, CommittedEvent::LathrilCombatPublishedV1 { .. })
                }),
                6 => {
                    let row = forged.stack.last_mut().unwrap();
                    row.kind = StackItemKind::ActivatedAbility;
                    row.v4.activated_ability_index = Some(0);
                    row.inline_effect = Some((card_def::CARD_DEFS[usize::from(id(NAME))]
                        .activated_abilities[0]
                        .effect)());
                }
                7 => forged.stack.last_mut().unwrap().v4.ability_source_contract = None,
                8 => forged.engine.event_history.retain(|entry| {
                    !matches!(entry, CommittedEvent::LathrilCombatObservedV1 { .. })
                }),
                9 => {
                    let marker = forged
                        .engine
                        .event_history
                        .iter_mut()
                        .find(|entry| matches!(entry, CommittedEvent::CombatDamageToPlayer { .. }))
                        .unwrap();
                    if let CommittedEvent::CombatDamageToPlayer { amount, .. } = marker {
                        *amount += 1;
                    }
                }
                _ => {
                    let publication = forged
                        .engine
                        .event_history
                        .iter()
                        .find(|entry| {
                            matches!(entry, CommittedEvent::LathrilCombatPublishedV1 { .. })
                        })
                        .unwrap()
                        .clone();
                    forged.engine.event_history.push(publication);
                }
            }
            let unchanged = forged.clone();
            assert!(engine::validated_stack_item_target_spec(
                forged.stack.last().unwrap(),
                &forged
            )
            .is_err());
            assert_eq!(forged, unchanged);
        }
        // A public proposal's editable is_combat flag cannot mint the genuine
        // private engine marker handoff or an executable Lathril occurrence.
        let mut isolated = state.clone();
        let mut packet = ProposedEvent::damage(source, Target::Player(actor.opponent()), 1);
        if let ProposedEvent::Damage(damage) = &mut packet {
            damage.is_combat = true;
        }
        event::propose_and_commit(&mut isolated, packet);
        let fake_capture = isolated
            .engine
            .event_history
            .iter()
            .rposition(|entry| matches!(entry, CommittedEvent::LathrilCombatDamageV1 { .. }))
            .unwrap() as u64;
        assert_ne!(fake_capture, capture);
        assert!(event::lathril_damage_v1(&isolated, fake_capture).is_err());
        assert_eq!(tokens(&isolated, actor), 0);
        assert_eq!(state, before);
        assert!(crate::optional_trigger_public_v1::validate_state(&state).is_err());
        let restored = restore(&state);
        assert_eq!(state, restored);
    }
}

#[test]
fn native_final_packet_prevention_season_suppression_noncombat_and_width_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..5 {
            let (mut state, source) = paid(actor);
            if mode == 0 {
                cast(&mut state, actor, "Doubling Season", None, 5);
                finish(&mut state);
            }
            if mode == 1 || mode == 2 {
                // Explicit replacement primitive, not a claim of paying a prevention spell.
                state.engine.active_replacements.push(ActiveReplacement {
                    id: 1,
                    source,
                    kind: ReplacementEffectKind::PreventNextDamage {
                        target: Target::Player(actor.opponent()),
                        remaining: if mode == 1 { 2 } else { 5 },
                    },
                });
            }
            if mode == 3 {
                // Explicit native suppression primitive, not a paid Tidebinder claim.
                let suppressor = put(&mut state, actor.opponent(), "Bear Cub", Zone::Battlefield);
                let link = crate::state::ObjectLinkV4 {
                    object: suppressor,
                    zone_change_count: state.objects.get(suppressor).zone_change_count,
                };
                state
                    .objects
                    .get_mut(source)
                    .v4
                    .creature_upgrade
                    .get_or_insert_with(Default::default)
                    .suppressed_by
                    .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                        source: link,
                        timestamp: 1,
                    });
            }
            if mode == 4 {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::damage(source, Target::Player(actor.opponent()), 5),
                );
            } else {
                // Explicit final-combat packet primitive, independent of printed power2.
                engine::commit_combat_damage_events(
                    &mut state,
                    vec![ProposedEvent::damage(
                        source,
                        Target::Player(actor.opponent()),
                        5,
                    )],
                );
            }
            finish(&mut state);
            assert_eq!(
                tokens(&state, actor),
                match mode {
                    0 => 10,
                    1 => 3,
                    _ => 0,
                }
            );
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|entry| matches!(entry, CommittedEvent::LathrilCombatDamageV1 { .. }))
                    .count(),
                usize::from(mode < 2)
            );
        }
        let (mut state, source) = paid(actor);
        state.players[actor.opponent().index()].life = i32::MAX; // explicit native width setup
        engine::commit_combat_damage_events(
            &mut state,
            vec![ProposedEvent::damage(
                source,
                Target::Player(actor.opponent()),
                70_000,
            )],
        );
        let capture = state
            .engine
            .event_history
            .iter()
            .position(|entry| matches!(entry, CommittedEvent::LathrilCombatDamageV1 { .. }))
            .unwrap() as u64;
        assert_eq!(event::lathril_damage_v1(&state, capture).unwrap().1, 70_000);
        // Capture width only; deliberately do not allocate seventy thousand tokens.
        assert_eq!(tokens(&state, actor), 0);
    }
}

#[test]
fn paid_self_reflection_copy_uses_its_own_original_combat_source_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        cast(
            &mut state,
            actor,
            "Self-Reflection",
            Some(Target::Object(source)),
            6,
        );
        finish(&mut state);
        let copy = state
            .objects
            .iter()
            .find_map(|(id, o)| {
                (id != source
                    && o.zone == Zone::Battlefield
                    && o.card_def == crate::card_def::card_id_by_name(NAME).unwrap())
                .then_some(id)
            })
            .unwrap();
        assert!(state.objects.get(copy).v4.is_token);
        assert_eq!(
            state.objects.get(source).zone,
            Zone::Graveyard,
            "genuine legend choice kept the copy"
        );
        publish_from_primitive(&mut state, copy, actor.opponent(), 2);
        let row = state.stack.last().unwrap();
        assert_eq!(row.source, copy);
        assert_eq!(row.v4.ability_source_contract.unwrap().source, copy);
        finish(&mut state);
        assert_eq!(tokens(&state, actor), 2);
    }
}

#[test]
fn paid_actual_menace_block_prevention_and_double_strike_combat_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let (mut state, source) = paid(actor);
            advance_to_next_main_for(&mut state, actor);
            crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
            // Explicit native blocker setup; Lathril's cast and all combat
            // declarations/assignments are actual paid/native actions.
            let blockers = [
                put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                ),
                put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                ),
            ];
            if mode == 1 {
                state.engine.active_replacements.push(ActiveReplacement {
                    id: 1,
                    source,
                    kind: ReplacementEffectKind::PreventNextDamage {
                        target: Target::Player(actor.opponent()),
                        remaining: 2,
                    },
                });
            }
            if mode == 2 {
                // Explicit native keyword grant, not a paid double-strike spell.
                state
                    .objects
                    .get_mut(source)
                    .v4
                    .creature_upgrade
                    .get_or_insert_with(Default::default)
                    .keyword_grants
                    .push((Keywords::DOUBLE_STRIKE, 1));
            }
            let mut attacked = false;
            let mut ended = false;
            for _ in 0..128 {
                let decision = engine::advance_until_decision(&mut state);
                assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
                if attacked && state.step == Step::EndCombat && state.stack.is_empty() {
                    ended = true;
                    break;
                }
                match decision {
                    Decision::DeclareAttackers { .. } => {
                        engine::step(&mut state, Action::DeclareAttackers(vec![source])).unwrap();
                        attacked = true;
                    }
                    Decision::DeclareBlockers { .. } => {
                        if mode == 0 {
                            let unchanged = state.clone();
                            assert!(
                                engine::step(
                                    &mut state,
                                    Action::DeclareBlockers(vec![(blockers[0], source)])
                                )
                                .is_err(),
                                "menace rejects one blocker"
                            );
                            assert_eq!(state, unchanged);
                        }
                        engine::step(
                            &mut state,
                            Action::DeclareBlockers(if mode == 0 {
                                blockers.iter().map(|&b| (b, source)).collect()
                            } else {
                                vec![]
                            }),
                        )
                        .unwrap();
                    }
                    Decision::ChooseCombatDamageRange { .. } => engine::step(
                        &mut state,
                        Action::ChooseCombatDamageRange { upper_half: false },
                    )
                    .unwrap(),
                    other => respond(&mut state, other),
                }
            }
            assert!(ended);
            assert_eq!(tokens(&state, actor), if mode == 2 { 4 } else { 0 });
            assert_eq!(
                state.players[actor.opponent().index()].life,
                if mode == 2 { 16 } else { 20 }
            );
            assert_eq!(
                state
                    .engine
                    .event_history
                    .iter()
                    .filter(|entry| matches!(entry, CommittedEvent::LathrilCombatResolvedV1 { .. }))
                    .count(),
                if mode == 2 { 2 } else { 0 }
            );
        }
    }
}
