//! Paid native FDN193 cases. Initial lands and recipients are support operands;
//! mana, casting, turn progression, attack and target answers use engine actions.
use super::*;
use crate::{
    card_def::{card_id_by_name, CardCapability, TargetSpec},
    engine::{self, Action, Decision},
    policy_observation_v6::tests::put,
    state::StackItemKind,
};

const NAME: &str = "Drakuseth, Maw of Flames";

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn quiet(state: &mut GameState, decision: Decision) {
    match decision {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::DeclareAttackers { .. } => {
            engine::step(state, Action::DeclareAttackers(Vec::new())).unwrap()
        }
        Decision::DeclareBlockers { .. } => {
            engine::step(state, Action::DeclareBlockers(Vec::new())).unwrap()
        }
        Decision::OrderTriggers { pending, .. } => {
            engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
        }
        other => panic!("unexpected actual native choice: {other:?}"),
    }
}
fn settle(state: &mut GameState) {
    for _ in 0..64 {
        let decision = next(state);
        if matches!(decision, Decision::CastSpellOrPass { .. })
            && state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(state, decision);
    }
    panic!("genuine Drakuseth operation did not settle");
}
fn restored(state: &GameState) -> GameState {
    let restored = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(&restored, state);
    restored
}
fn paid(actor: PlayerId) -> (GameState, ObjectId) {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    let source = put(&mut state, actor, NAME, Zone::Hand);
    let lands = (0..7)
        .map(|_| put(&mut state, actor, "Mountain", Zone::Battlefield))
        .collect::<Vec<_>>();
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    for &land in &lands {
        engine::step(&mut state, Action::ActivateManaAbility(land)).unwrap();
    }
    assert_eq!(
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()],
        7
    );
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    assert!(lands.iter().all(|land| state.objects.get(*land).tapped));
    assert!(state.engine.event_history.iter().any(|event| matches!(event,
        CommittedEvent::SpellCast { spell, controller } if *spell == source && *controller == actor)));
    settle(&mut state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    (state, source)
}
fn attack(state: &mut GameState, source: ObjectId, actor: PlayerId) {
    let cast_turn = state.turn_key_v1();
    let generation = state.objects.get(source).zone_change_count;
    let mut saw_sick_declaration = false;
    for _ in 0..512 {
        let decision = next(state);
        if let Decision::DeclareAttackers { player, eligible } = &decision {
            if *player == actor {
                if state.turn_key_v1() == cast_turn {
                    assert!(!eligible.contains(&source));
                    saw_sick_declaration = true;
                } else if eligible.contains(&source) {
                    assert!(saw_sick_declaration);
                    engine::step(state, Action::DeclareAttackers(vec![source])).unwrap();
                    assert_eq!(state.engine.combat.attackers.as_slice(), &[source]);
                    assert!(state.engine.event_history.iter().any(|event| matches!(event,
                        CommittedEvent::DeclaredAttacker { source: actual, source_zone_change_count, controller }
                        if *actual == source && *source_zone_change_count == generation && *controller == actor)));
                    for _ in 0..16 {
                        let decision = next(state);
                        if matches!(decision, Decision::ChooseTargets { player, spell, remaining: 3, can_finish: false, .. }
                            if player == actor && spell == source)
                        {
                            return;
                        }
                        quiet(state, decision);
                    }
                    panic!("genuine attack did not publish mandatory first target");
                }
            }
        }
        quiet(state, decision);
    }
    panic!("genuinely cast source did not reach its next legal attack");
}
fn choose(state: &mut GameState, source: ObjectId, targets: &[Target]) {
    for (index, target) in targets.iter().enumerate() {
        assert!(
            matches!(next(state), Decision::ChooseTargets { spell, legal_targets, can_finish, .. }
            if spell == source && legal_targets.contains(target) && can_finish == (index != 0))
        );
        engine::step(state, Action::ChooseTarget(*target)).unwrap();
    }
    if targets.len() < 3 {
        assert!(matches!(
            next(state),
            Decision::ChooseTargets {
                can_finish: true,
                ..
            }
        ));
        engine::step(state, Action::FinishEffectSelection).unwrap();
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    let row = state.stack.last().unwrap();
    assert_eq!(row.kind, StackItemKind::TriggeredAbility);
    assert_eq!(row.targets.as_slice(), targets);
    assert_eq!(row.v4.target_contracts.len(), targets.len());
    assert_eq!(row.inline_effect, Some(drakuseth_attack_v1()));
    assert_eq!(
        row.v4.target_spec,
        Some(TargetSpec::StandardV1(
            StandardTargetV1::OneToThreeDistinctAnyTargetsV1
        ))
    );
}
fn damage_since(state: &GameState, start: usize, source: ObjectId) -> Vec<(Target, i32)> {
    state.engine.event_history[start..]
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::Damage {
                source: actual,
                target,
                amount,
            } if *actual == source => Some((*target, *amount)),
            _ => None,
        })
        .collect()
}

#[test]
fn paid_attack_mandatory_first_zero_one_two_other_targets_and_every_target_checkpoint_restore_both_seats(
) {
    let id = card_id_by_name(NAME).unwrap();
    assert_eq!(id, 464);
    let definition = &CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!(
        (
            definition.cost.generic,
            definition.cost.pips.len(),
            definition.mana_value
        ),
        (4, 3, 7)
    );
    assert!(definition.keywords.has(Keywords::FLYING));
    assert_eq!((definition.power, definition.toughness), (Some(7), Some(7)));
    assert!(crate::card_def::preflight_fully_supported_deck(&[id]).is_err());
    for actor in [PlayerId::P0, PlayerId::P1] {
        for count in 1..=3 {
            let (mut state, source) = paid(actor);
            let other = [
                put(
                    &mut state,
                    actor.opponent(),
                    "Serra Angel",
                    Zone::Battlefield,
                ),
                put(
                    &mut state,
                    actor.opponent(),
                    "Serra Angel",
                    Zone::Battlefield,
                ),
            ];
            attack(&mut state, source, actor);
            let before = state.clone();
            assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
            assert_eq!(state, before);
            let targets = [
                Target::Player(actor.opponent()),
                Target::Object(other[0]),
                Target::Object(other[1]),
            ];
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                // Preserve every actual intermediate target-selection checkpoint.
                for (index, target) in targets[..count].iter().enumerate() {
                    assert!(
                        matches!(next(branch), Decision::ChooseTargets { legal_targets, can_finish, .. }
                        if legal_targets.contains(target) && can_finish == (index != 0))
                    );
                    let saved = restored(branch);
                    engine::step(branch, Action::ChooseTarget(*target)).unwrap();
                    let mut alternative = saved;
                    engine::step(&mut alternative, Action::ChooseTarget(*target)).unwrap();
                    assert_eq!(*branch, alternative);
                    if index + 1 < 3 {
                        let decision = next(branch);
                        assert!(
                            matches!(decision, Decision::ChooseTargets { legal_targets, .. } if !legal_targets.contains(target))
                        );
                        let before = branch.clone();
                        assert!(engine::step(branch, Action::ChooseTarget(*target)).is_err());
                        assert_eq!(*branch, before);
                    }
                }
                if count < 3 {
                    engine::step(branch, Action::FinishEffectSelection).unwrap();
                }
                assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                let row = branch.stack.last().unwrap();
                assert_eq!(row.targets, targets[..count]);
                assert_eq!(row.controller, actor);
                let start = branch.engine.event_history.len();
                let mut published_restore = restored(branch);
                settle(branch);
                settle(&mut published_restore);
                assert_eq!(*branch, published_restore);
                assert_eq!(
                    damage_since(branch, start, source),
                    targets[..count]
                        .iter()
                        .enumerate()
                        .map(|(index, target)| (*target, if index == 0 { 4 } else { 3 }))
                        .collect::<Vec<_>>()
                );
                assert_eq!(branch.players[actor.opponent().index()].life, 16);
            }
            assert_eq!(state, replay);
        }
    }
}

#[test]
fn paid_attack_individual_illegality_never_compacts_original_damage_slots_and_all_illegal_fizzles_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for removed in [vec![0], vec![1], vec![0, 1, 2]] {
            let (mut state, source) = paid(actor);
            let recipients = (0..3)
                .map(|_| {
                    put(
                        &mut state,
                        actor.opponent(),
                        "Serra Angel",
                        Zone::Battlefield,
                    )
                })
                .collect::<Vec<_>>();
            attack(&mut state, source, actor);
            let targets = recipients
                .iter()
                .copied()
                .map(Target::Object)
                .collect::<Vec<_>>();
            choose(&mut state, source, &targets);
            let published = state.clone();
            let mut replay = restored(&published);
            for branch in [&mut state, &mut replay] {
                // Labelled native departure/reentry response primitives. These
                // are not claims that a paid bounce spell was cast.
                for &index in &removed {
                    event::propose_and_commit(
                        branch,
                        ProposedEvent::zone_change(recipients[index], Zone::Hand),
                    );
                    event::propose_and_commit(
                        branch,
                        ProposedEvent::zone_change(recipients[index], Zone::Battlefield),
                    );
                }
                let start = branch.engine.event_history.len();
                settle(branch);
                let expected = targets
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !removed.contains(index))
                    .map(|(index, target)| (*target, if index == 0 { 4 } else { 3 }))
                    .collect::<Vec<_>>();
                assert_eq!(damage_since(branch, start, source), expected);
            }
            assert_eq!(state, replay);
        }
    }
}

#[test]
fn paid_attack_departed_reentered_and_stolen_source_retains_original_controller_and_target_contracts_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        attack(&mut state, source, actor);
        choose(&mut state, source, &[Target::Player(actor.opponent())]);
        let captured = state
            .stack
            .last()
            .unwrap()
            .v4
            .ability_source_contract
            .unwrap();
        assert_eq!(captured.controller, actor);
        let targets = state.stack.last().unwrap().v4.target_contracts.clone();
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            // Real control/departure primitives on an already paid/attacked
            // source never change the independent published trigger controller.
            gain_control(branch, source, actor.opponent());
            event::propose_and_commit(branch, ProposedEvent::zone_change(source, Zone::Hand));
            event::propose_and_commit(
                branch,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert!(branch.objects.get(source).zone_change_count > captured.zone_change_count);
            assert_eq!(
                branch.stack.last().unwrap().v4.ability_source_contract,
                Some(captured)
            );
            assert_eq!(branch.stack.last().unwrap().v4.target_contracts, targets);
            assert_eq!(branch.stack.last().unwrap().controller, actor);
            let start = branch.engine.event_history.len();
            settle(branch);
            assert_eq!(
                damage_since(branch, start, source),
                vec![(Target::Player(actor.opponent()), 4)]
            );
            assert_eq!(branch.players[actor.opponent().index()].life, 16);
        }
        assert_eq!(state, replay);
    }
}

fn mixed_prevention_scene(
    actor: PlayerId,
    form: u8,
) -> (GameState, ObjectId, [ObjectId; 2], usize) {
    let (mut state, source) = paid(actor);
    // Registered support operands. Only Drakuseth has a paid-cast claim here.
    put(&mut state, actor, "Crystal Barricade", Zone::Battlefield);
    put(&mut state, actor, "Twinflame Tyrant", Zone::Battlefield);
    let recipients =
        [actor, actor.opponent()].map(|p| put(&mut state, p, "Serra Angel", Zone::Battlefield));
    attack(&mut state, source, actor);
    choose(
        &mut state,
        source,
        &[
            Target::Player(actor.opponent()),
            Target::Object(recipients[0]),
            Target::Object(recipients[1]),
        ],
    );
    native_keyword_operand(&mut state, source, source, Keywords::LIFELINK);
    if form != 0 {
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        if form == 2 {
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert!(!engine::has_effective_keyword(
                &state,
                source,
                Keywords::LIFELINK
            ));
        }
    }
    // Actual finite shield primitives installed after turn progression.
    state.engine.next_replacement_id = 3;
    for (id, target, remaining) in [
        (1, Target::Player(actor.opponent()), 2),
        (2, Target::Object(recipients[0]), 2),
        (3, Target::Object(recipients[1]), 1),
    ] {
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id,
                source: match target {
                    Target::Object(o) => o,
                    _ => source,
                },
                kind: crate::event::ReplacementEffectKind::PreventNextDamage { target, remaining },
            });
    }
    let start = state.engine.event_history.len();
    (state, source, recipients, start)
}
fn pending_prevention(state: &GameState) -> &crate::effect::crystal_prevention_v1::ChoiceV1 {
    let Some(crate::effect::PendingEffectChoice::ChooseOption {
        purpose: crate::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|p| p.choice.as_ref())
    else {
        panic!("actual native prevention choice absent")
    };
    choice
}
fn finish_mixed_prevention(
    state: &mut GameState,
    source: ObjectId,
    recipients: [ObjectId; 2],
    start: usize,
    multiply_first: bool,
) -> Vec<(Target, PlayerId)> {
    use crate::effect::crystal_prevention_v1::PreventionV1;
    let mut seen = Vec::new();
    for _ in 0..64 {
        let decision = next(state);
        if let Decision::ChooseEffectOption {
            player,
            source: actual,
            option_count,
        } = decision
        {
            assert_eq!(actual, source);
            crate::effect::validate_pending_effect_choice(state).unwrap();
            crate::drakuseth_damage_source_v76::validate_state(state).unwrap();
            let choice = pending_prevention(state).clone();
            assert!(choice.batch.is_some());
            assert_eq!(choice.chooser, player);
            assert_eq!(usize::from(option_count), choice.options.len());
            if seen.last() != Some(&(choice.target, player)) {
                seen.push((choice.target, player));
            }
            assert!(
                damage_since(state, start, source).is_empty(),
                "no recipient commits before all replacements are selected"
            );
            assert_eq!(state.players[0].life, 20);
            assert_eq!(state.players[1].life, 20);
            assert!(recipients.iter().all(|&o| state.objects.get(o).damage == 0
                && state.objects.get(o).zone == Zone::Battlefield));
            let saved = restored(state);
            assert_eq!(&saved, state);
            *state = saved;
            let preferred = choice
                .options
                .iter()
                .position(|o| {
                    if multiply_first {
                        matches!(o, PreventionV1::Multiplier(_) | PreventionV1::Barricade(_))
                    } else {
                        matches!(o, PreventionV1::Replacement(_))
                    }
                })
                .unwrap_or(0);
            engine::step(
                state,
                Action::ChooseEffectOption(u16::try_from(preferred).unwrap()),
            )
            .unwrap();
        } else if state.stack.is_empty() && state.engine.pending_effect.is_none() {
            return seen;
        } else {
            quiet(state, decision)
        }
    }
    panic!("mixed original positional damage did not finish")
}
#[test]
fn paid_attack_mixed_player_permanent_prevention_apnap_single_batch_source_lki_and_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for form in 0..3 {
            for multiply_first in [false, true] {
                let (mut state, source, recipients, start) = mixed_prevention_scene(actor, form);
                let mut replay = restored(&state);
                for branch in [&mut state, &mut replay] {
                    let seen =
                        finish_mixed_prevention(branch, source, recipients, start, multiply_first);
                    assert_eq!(
                        seen,
                        vec![
                            (Target::Object(recipients[0]), actor),
                            (Target::Player(actor.opponent()), actor.opponent()),
                            (Target::Object(recipients[1]), actor.opponent())
                        ]
                    );
                    let player_damage = if multiply_first { 6 } else { 4 };
                    let creature_damage = if multiply_first { 5 } else { 4 };
                    assert_eq!(
                        damage_since(branch, start, source),
                        vec![
                            (Target::Player(actor.opponent()), player_damage),
                            (Target::Object(recipients[1]), creature_damage)
                        ]
                    );
                    assert_eq!(branch.objects.get(recipients[0]).zone, Zone::Battlefield);
                    assert_eq!(branch.objects.get(recipients[0]).damage, 0);
                    assert_eq!(branch.objects.get(recipients[1]).zone, Zone::Graveyard);
                    assert_eq!(
                        branch.players[actor.opponent().index()].life,
                        20 - player_damage
                    );
                    assert_eq!(
                        branch.players[actor.index()].life,
                        20 + player_damage + creature_damage
                    );
                    let shield = branch.engine.active_replacements.iter().find(|r| r.id == 2);
                    assert_eq!(shield.is_some(), multiply_first);
                    assert_eq!(
                        branch.engine.active_replacements.len(),
                        usize::from(multiply_first)
                    );
                    crate::effect::crystal_prevention_v1::validate(branch).unwrap();
                    crate::drakuseth_damage_source_v76::validate_state(branch).unwrap();
                }
                assert_eq!(state, replay);
            }
        }
    }
}
#[test]
fn paid_attack_player_finite_shield_choice_without_crystal_keeps_real_remaining_shields_and_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        attack(&mut state, source, actor);
        choose(&mut state, source, &[Target::Player(actor.opponent())]);
        state.engine.next_replacement_id = 2;
        for (id, remaining) in [(1, 3), (2, 5)] {
            state
                .engine
                .active_replacements
                .push(crate::event::ActiveReplacement {
                    id,
                    source,
                    kind: crate::event::ReplacementEffectKind::PreventNextDamage {
                        target: Target::Player(actor.opponent()),
                        remaining,
                    },
                });
        }
        let saved = restored(&state);
        let start = state.engine.event_history.len();
        for first_id in [1, 2] {
            let mut expected = None;
            for _ in 0..2 {
                let mut branch = restored(&saved);
                let mut first = true;
                for _ in 0..24 {
                    let d = next(&mut branch);
                    if let Decision::ChooseEffectOption { player, .. } = d {
                        assert_eq!(player, actor.opponent());
                        let choice = pending_prevention(&branch);
                        assert_eq!(choice.target, Target::Player(actor.opponent()));
                        let index = if first {
                            choice.options.iter().position(|o|matches!(o,crate::effect::crystal_prevention_v1::PreventionV1::Replacement(r)if r.id==first_id)).unwrap()
                        } else {
                            0
                        };
                        first = false;
                        branch = restored(&branch);
                        engine::step(
                            &mut branch,
                            Action::ChooseEffectOption(u16::try_from(index).unwrap()),
                        )
                        .unwrap();
                    } else if branch.stack.is_empty() {
                        break;
                    } else {
                        quiet(&mut branch, d)
                    }
                }
                assert!(!first);
                assert!(branch.stack.is_empty());
                assert!(damage_since(&branch, start, source).is_empty());
                assert_eq!(branch.players[actor.opponent().index()].life, 20);
                let remaining = branch
                    .engine
                    .active_replacements
                    .iter()
                    .map(|r| match r.kind {
                        crate::event::ReplacementEffectKind::PreventNextDamage {
                            remaining,
                            ..
                        } => (r.id, remaining),
                        _ => panic!(),
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    remaining,
                    if first_id == 1 {
                        vec![(2, 4)]
                    } else {
                        vec![(1, 3), (2, 1)]
                    }
                );
                crate::effect::crystal_prevention_v1::validate(&branch).unwrap();
                crate::drakuseth_damage_source_v76::validate_state(&branch).unwrap();
                if let Some(ref old) = expected {
                    assert_eq!(&branch, old)
                } else {
                    expected = Some(branch)
                }
            }
        }
    }
}

#[test]
fn paid_mixed_batch_forged_player_roster_life_positional_amount_or_chooser_refuses_before_damage_both_seats(
) {
    use crate::effect::crystal_prevention_v1::BatchCaptureV2;
    fn change(capture: &mut BatchCaptureV2, field: u8) {
        match field {
            0 => capture.players_v76 = None,
            1 => capture.players_v76.as_mut().unwrap()[0].life += 1,
            2 => capture.players_v76.as_mut().unwrap()[0].player = PlayerId::P1,
            3 => capture.proposals[0].amount += 1,
            _ => {}
        }
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, recipients, start) = mixed_prevention_scene(actor, 2);
        let mut reached = false;
        for _ in 0..16 {
            let d = next(&mut state);
            if matches!(d, Decision::ChooseEffectOption { .. }) {
                reached = true;
                break;
            }
            quiet(&mut state, d);
        }
        assert!(reached);
        crate::effect::validate_pending_effect_choice(&state).unwrap();
        for field in 0..5 {
            let mut forged = restored(&state);
            for e in forged.engine.event_history.iter_mut() {
                match e {
                    CommittedEvent::CrystalBatchPreparedV3 { capture } => change(capture, field),
                    CommittedEvent::CrystalExecutionV1 { execution } => {
                        for step in &mut execution.steps {
                            if let Some(capture) = step.batch_capture.as_mut() {
                                change(capture, field)
                            }
                        }
                    }
                    _ => {}
                }
            }
            if field == 4 {
                let Some(crate::effect::PendingEffectChoice::ChooseOption {
                    purpose:
                        crate::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                    ..
                }) = forged
                    .engine
                    .pending_effect
                    .as_mut()
                    .unwrap()
                    .choice
                    .as_mut()
                else {
                    panic!()
                };
                choice.chooser = choice.chooser.opponent();
            }
            assert!(
                crate::effect::validate_pending_effect_choice(&forged).is_err(),
                "forged mixed field {field}"
            );
            let before = forged.engine.event_history.len();
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted { .. }
            ));
            assert_eq!(forged.engine.event_history.len(), before);
            assert!(damage_since(&forged, start, source).is_empty());
            assert!(recipients
                .iter()
                .all(|&o| forged.objects.get(o).damage == 0));
            assert_eq!(forged.players[0].life, 20);
            assert_eq!(forged.players[1].life, 20);
        }
    }
}

#[test]
fn paid_attack_serialized_context_source_and_controller_forgery_refuse_without_events_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        let foreign = put(&mut state, actor, "Forest", Zone::Battlefield);
        attack(&mut state, source, actor);
        choose(&mut state, source, &[Target::Player(actor.opponent())]);
        let row = state.stack.last().unwrap();
        let captured = row.v4.ability_source_contract.unwrap();
        assert_eq!((captured.source, captured.controller), (source, actor));
        // Copy only the published row's public execution fields. No private
        // damage origin is invented; genuine execution below uses the engine.
        let mut ctx = ExecCtx::no_targets(row.source, row.controller);
        ctx.stack_item_id = Some(row.v4.stack_item_id);
        ctx.targets = row.targets.clone();
        ctx.target_contracts = row.v4.target_contracts.clone();
        ctx.discarded = row.discarded.clone();
        ctx.paid_cost_refs = row.v4.paid_cost_refs.clone();
        ctx.hidden_ability_source = row.v4.hidden_ability_source;
        ctx.ability_source_contract = row.v4.ability_source_contract;
        ctx.cauldron_grant = row.v4.cauldron_grant;
        ctx.kicked = row.kicked;
        ctx.optional_additional_cost_paid = row.v4.optional_additional_cost_paid;
        ctx.x_value = row.v4.x_value;
        for change_source in [true, false] {
            let mut forged = ctx.clone();
            if change_source {
                forged.source = foreign;
            } else {
                forged.controller = actor.opponent();
            }
            let forged: ExecCtx =
                serde_json::from_slice(&serde_json::to_vec(&forged).unwrap()).unwrap();
            assert_eq!(forged.ability_source_contract, Some(captured));
            let mut branch = restored(&state);
            let before = branch.clone();
            crate::effect::execute(&drakuseth_attack_v1(), &forged, &mut branch);
            assert_eq!(branch, before);
            assert!(damage_since(&branch, before.engine.event_history.len(), source).is_empty());
        }
        let mut replay = restored(&state);
        let start = state.engine.event_history.len();
        for branch in [&mut state, &mut replay] {
            settle(branch);
            assert_eq!(
                damage_since(branch, start, source),
                vec![(Target::Player(actor.opponent()), 4)]
            );
            assert_eq!(branch.players[actor.opponent().index()].life, 16);
        }
        assert_eq!(state, replay);
    }
}

fn native_keyword_operand(
    state: &mut GameState,
    source: ObjectId,
    target: ObjectId,
    keyword: Keywords,
) {
    // A labelled native support effect, not a claim that a granting spell was cast.
    let mut ctx = ExecCtx::no_targets(source, state.objects.get(source).controller);
    ctx.targets = vec![Target::Object(target)];
    ctx.target_contracts = vec![crate::state::StackTargetContractV4::capture(
        state,
        Target::Object(target),
    )];
    crate::effect::execute(
        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: crate::effect::ObjectRef::Target(0),
            keyword,
        },
        &ctx,
        state,
    );
}

#[test]
fn paid_attack_live_departed_reentered_multicolor_lifelink_deathtouch_original_damage_batch_and_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for stolen in [false, true] {
            for form in 0..3 {
                let (mut state, source) = paid(actor);
                let recipients = [0, 1].map(|_| {
                    put(
                        &mut state,
                        actor.opponent(),
                        "Serra Angel",
                        Zone::Battlefield,
                    )
                });
                attack(&mut state, source, actor);
                let targets = [
                    Target::Player(actor.opponent()),
                    Target::Object(recipients[0]),
                    Target::Object(recipients[1]),
                ];
                choose(&mut state, source, &targets);
                let row = state.stack.last().unwrap().clone();
                let id = row.v4.stack_item_id;
                let original = row.v4.ability_source_contract.unwrap();
                // Materialized native color is a support operand. There is no
                // paid color-changing-spell claim in this source-LKI test.
                let colors = crate::card_def::mana_colors_mask(&[ManaColor::R, ManaColor::U]);
                state.objects.get_mut(source).v4.effective_color_mask = colors;
                native_keyword_operand(
                    &mut state,
                    source,
                    source,
                    Keywords(Keywords::LIFELINK.0 | Keywords::DEATHTOUCH.0),
                );
                native_keyword_operand(
                    &mut state,
                    source,
                    recipients[0],
                    Keywords::PROTECTION_FROM_MONOCOLORED,
                );
                if stolen {
                    gain_control(&mut state, source, actor.opponent());
                }
                let controller = if stolen { actor.opponent() } else { actor };
                let captured =
                    crate::source_power_activation_v1::capture_damage_source(&state, source);
                assert_eq!(captured.color_mask, colors);
                assert!(captured.lifelink && captured.deathtouch);
                assert_eq!(captured.source.controller, controller);
                if form != 0 {
                    event::propose_and_commit(
                        &mut state,
                        ProposedEvent::zone_change(source, Zone::Hand),
                    );
                    if form == 2 {
                        event::propose_and_commit(
                            &mut state,
                            ProposedEvent::zone_change(source, Zone::Battlefield),
                        );
                        assert!(
                            state.objects.get(source).zone_change_count
                                > original.zone_change_count
                        );
                        assert_eq!(
                            engine::object_color_mask(&state, source),
                            crate::card_def::mana_colors_mask(&[ManaColor::R])
                        );
                        assert!(!engine::has_effective_keyword(
                            &state,
                            source,
                            Keywords::LIFELINK
                        ));
                    }
                }
                assert_eq!(
                    crate::drakuseth_damage_source_v76::damage_source(&state, source, id).unwrap(),
                    captured
                );
                crate::drakuseth_damage_source_v76::validate_state(&state).unwrap();
                let mut replay = restored(&state);
                let start = state.engine.event_history.len();
                for branch in [&mut state, &mut replay] {
                    settle(branch);
                    assert_eq!(
                        damage_since(branch, start, source),
                        vec![(targets[0], 4), (targets[1], 3), (targets[2], 3)]
                    );
                    assert!(recipients
                        .iter()
                        .all(|&o| branch.objects.get(o).zone == Zone::Graveyard));
                    assert_eq!(
                        branch.players[actor.index()].life,
                        if stolen { 20 } else { 30 }
                    );
                    assert_eq!(
                        branch.players[actor.opponent().index()].life,
                        if stolen { 26 } else { 16 }
                    );
                    let interval = &branch.engine.event_history[start..];
                    assert_eq!(interval.iter().filter(|e|matches!(e,CommittedEvent::LifeGain{player,amount}if *player==controller&&*amount==10)).count(),1);
                    let last_damage = interval
                        .iter()
                        .rposition(|e| matches!(e,CommittedEvent::Damage{source:s,..}if *s==source))
                        .unwrap();
                    let first_death=interval.iter().position(|e|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Battlefield,to:Zone::Graveyard,..}if recipients.contains(object))).unwrap();
                    assert!(last_damage < first_death);
                    crate::drakuseth_damage_source_v76::validate_state(branch).unwrap();
                }
                assert_eq!(state, replay);
            }
        }
    }
}

#[test]
fn paid_attack_publication_departure_marker_forgeries_and_serialized_unowned_context_refuse_both_seats(
) {
    use crate::drakuseth_damage_source_v76 as owned;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        attack(&mut state, source, actor);
        choose(&mut state, source, &[Target::Player(actor.opponent())]);
        let row = state.stack.last().unwrap().clone();
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.stack_item_id = Some(row.v4.stack_item_id);
        ctx.ability_source_contract = row.v4.ability_source_contract;
        ctx.targets = row.targets.clone();
        ctx.target_contracts = row.v4.target_contracts.clone();
        let ctx: ExecCtx = serde_json::from_slice(&serde_json::to_vec(&ctx).unwrap()).unwrap();
        let before = state.clone();
        crate::effect::execute(&drakuseth_attack_v1(), &ctx, &mut state);
        assert_eq!(
            state, before,
            "matching serialized fields are not a native removal loan"
        );
        for field in 0..4 {
            let mut forged = restored(&state);
            match field {
                0=>forged.engine.event_history.retain(|e|!matches!(e,CommittedEvent::DrakusethDamageSourceV76{journal}if matches!(journal.as_ref(),owned::JournalV76::Published{..}))),
                1=>forged.stack.last_mut().unwrap().v4.drakuseth_published_v76.0=None,
                2=>forged.stack.last_mut().unwrap().targets[0]=Target::Player(actor),
                _=>{
                    let duplicate=forged.engine.event_history.iter().find(|e|matches!(e,CommittedEvent::DrakusethDamageSourceV76{journal}if matches!(journal.as_ref(),owned::JournalV76::Published{..}))).unwrap().clone();
                    forged.engine.event_history.push(duplicate);
                }
            }
            assert!(owned::validate_state(&forged).is_err());
            let count = forged.engine.event_history.len();
            assert!(matches!(
                engine::advance_until_decision(&mut forged),
                Decision::Halted {
                    mechanic: engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ..
                }
            ));
            assert_eq!(forged.engine.event_history.len(), count);
        }
        event::propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
        owned::validate_state(&state).unwrap();
        let mut forged = restored(&state);
        forged.engine.event_history.retain(|e|!matches!(e,CommittedEvent::DrakusethDamageSourceV76{journal}if matches!(journal.as_ref(),owned::JournalV76::Departure{..})));
        assert!(owned::validate_state(&forged).is_err());
        let count = forged.engine.event_history.len();
        assert!(matches!(
            engine::advance_until_decision(&mut forged),
            Decision::Halted { .. }
        ));
        assert_eq!(forged.engine.event_history.len(), count);
    }
}

#[test]
fn paid_attack_native_counter_and_all_illegal_target_close_original_publication_once_and_restore_both_seats(
) {
    use crate::drakuseth_damage_source_v76 as owned;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = paid(actor);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Serra Angel",
            Zone::Battlefield,
        );
        attack(&mut state, source, actor);
        choose(&mut state, source, &[Target::Object(recipient)]);
        let saved = restored(&state);
        let start = state.engine.event_history.len();
        for countered in [false, true] {
            let mut expected = None;
            for _ in 0..2 {
                let mut branch = restored(&saved);
                if countered {
                    // Labelled native stack-counter primitive, not Counterspell
                    // targeting an ability or a paid countering-spell claim.
                    let id = branch.stack.last().unwrap().v4.stack_item_id;
                    engine::counter_stack_item_by_id(&mut branch, id).unwrap();
                } else {
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(recipient, Zone::Hand),
                    );
                    settle(&mut branch);
                }
                assert!(branch.stack.is_empty());
                assert!(damage_since(&branch, start, source).is_empty());
                owned::validate_state(&branch).unwrap();
                assert_eq!(branch.engine.event_history[start..].iter().filter(|e|matches!(e,
                    CommittedEvent::DrakusethDamageSourceV76{journal}if matches!(journal.as_ref(),owned::JournalV76::Removed{outcome,..}
                    if *outcome==if countered{owned::OutcomeV76::Countered}else{owned::OutcomeV76::NoLegalTarget}))).count(),1);
                if let Some(ref old) = expected {
                    assert_eq!(&branch, old);
                } else {
                    expected = Some(branch);
                }
            }
        }
    }
}
