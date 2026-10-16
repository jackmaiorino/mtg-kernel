//! Genuine Partial Drake Hatcher source. Consumer qualification is deferred.
use crate::{
    effect::{EffectOp, ExecCtx},
    ids::ObjectId,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};
pub mod custody;

/// Exact source counter family, separate from net/charge and distributed costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceCounterKindV1 {
    Incubation,
}

fn count(state: &GameState, source: ObjectId, kind: SourceCounterKindV1) -> Option<i32> {
    let live = state.objects.try_get(source)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    Some(match kind {
        SourceCounterKindV1::Incubation => live.counters.incubation,
    })
}
pub(crate) fn can_pay(
    state: &GameState,
    source: ObjectId,
    kind: SourceCounterKindV1,
    amount: u16,
) -> bool {
    amount > 0 && count(state, source, kind).is_some_and(|n| n >= i32::from(amount))
}
pub(crate) fn pay(
    state: &mut GameState,
    source: ObjectId,
    kind: SourceCounterKindV1,
    amount: u16,
) -> Option<()> {
    if !can_pay(state, source, kind, amount) {
        return None;
    }
    let before = count(state, source, kind)?;
    let after = before.checked_sub(i32::from(amount))?;
    custody::debit(state, source, kind, amount, before, after).ok()?;
    match kind {
        SourceCounterKindV1::Incubation => {
            state.objects.get_mut(source).counters.incubation = after
        }
    }
    Some(())
}
pub fn combat_binder() -> EffectOp {
    EffectOp::IncubationFromCombatDamageV1 { amount: None }
}
pub fn create_drake() -> EffectOp {
    EffectOp::DrakeCreateTokenFromPaidV1
}
/// The trigger freezes the final committed event amount. The source contract
/// prevents counters crossing a leave/reenter boundary. Replacement applies in
/// the resolution world; the ability's controller remains the effect's actor.
pub(crate) fn place(
    state: &mut GameState,
    ctx: &ExecCtx,
    amount: Option<i32>,
) -> Result<(), String> {
    let amount = amount
        .filter(|n| *n > 0)
        .ok_or("incubation requires a positive bound combat damage amount")?;
    let source = ctx
        .ability_source_contract
        .ok_or("incubation lacks original source")?;
    if source.source != ctx.source
        || source.controller != ctx.controller
        || source.zone != Zone::Battlefield
    {
        return Err("incubation source relation changed".into());
    }
    let live = state
        .objects
        .try_get(source.source)
        .ok_or("incubation source missing")?;
    if live.owner != source.owner
        || live.zone_change_count < source.zone_change_count
        || live.counters.incubation < 0
    {
        return Err("invalid incubation source/count".into());
    }
    if live.zone != Zone::Battlefield || live.zone_change_count != source.zone_change_count {
        return Ok(());
    }
    let amount =
        crate::standard_cards_v1::scale_counters_checked_v48(state, ctx.controller, amount)?;
    let amount = crate::doubling_season_v48::ReplacementInputV48::capture(state, live.controller)
        .replaced_count(amount)?;
    if amount < 0 {
        return Err("negative incubation placement".into());
    }
    let after = live
        .counters
        .incubation
        .checked_add(amount)
        .ok_or("incubation overflow")?;
    state.objects.get_mut(source.source).counters.incubation = after;
    Ok(())
}
/// Frozen public schemas have no incubation field. Refuse before omission.
pub(crate) fn require_public_counter(state: &GameState, id: ObjectId) -> Result<(), String> {
    if !state.engine.drake_ledger_v1.is_empty()
        || state.objects.try_get(id).is_some_and(|o| {
            o.counters.incubation != 0
                || (o.v4.face_down_v1.is_none()
                    && crate::card_def::CARD_DEFS
                        .get(usize::from(o.card_def))
                        .is_some_and(|d| d.name == "Drake Hatcher"))
        })
    {
        Err("public counter schema cannot represent exact incubation counters; Drake successor required".into())
    } else {
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::{
        engine::{self, Action, Decision},
        event::{self, ProposedEvent},
        ids::PlayerId,
        mana::ManaColor,
        state::{AbilitySourceContractV4, Target},
    };

    fn finish(state: &mut GameState) {
        for _ in 0..32 {
            match engine::advance_until_decision(state) {
                Decision::OrderTriggers { pending, .. } => engine::step(
                    state,
                    Action::OrderTriggers((0..pending.len()).rev().collect()),
                )
                .unwrap(),
                Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
                Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
                other => panic!("Drake stack: {other:?}"),
            }
        }
        panic!("Drake stack did not finish");
    }
    fn cast(actor: PlayerId) -> (GameState, ObjectId) {
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            for _ in 0..8 {
                put(&mut state, player, "Island", Zone::Library);
            }
        }
        let drake = put(&mut state, actor, "Drake Hatcher", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(drake)).unwrap();
        finish(&mut state);
        assert_eq!(state.objects.get(drake).zone, Zone::Battlefield);
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()],
            0
        );
        assert_eq!(
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
            0
        );
        (state, drake)
    }
    #[test]
    fn actual_paid_printing_damage_trigger_activation_restore_and_counter_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            let spell = put(&mut state, actor, "Think Twice", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            finish(&mut state);
            assert_eq!(engine::effective_power(&state, drake), 2);
            // Use the production combat damage commit boundary. It records final
            // damage and queues the real registered trigger, rather than making
            // a pending trigger or inserting an inline program in this fixture.
            engine::commit_combat_damage_events(
                &mut state,
                vec![ProposedEvent::damage(
                    drake,
                    Target::Player(actor.opponent()),
                    3,
                )],
            );
            finish(&mut state);
            assert_eq!(state.players[actor.opponent().index()].life, 17);
            assert_eq!(state.objects.get(drake).counters.incubation, 3);
            assert!(require_public_counter(&state, drake).is_err());
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                branch.priority_player = actor;
                engine::step(branch, Action::ActivateAbility(drake, 0)).unwrap();
                let _ = engine::advance_until_decision(branch);
                assert_eq!(branch.objects.get(drake).counters.incubation, 0);
                let item = branch
                    .stack
                    .last()
                    .expect("paid real activation")
                    .v4
                    .stack_item_id;
                let mut countered = branch.clone();
                assert!(engine::counter_stack_item_by_id(&mut countered, item)
                    .unwrap()
                    .is_some());
                assert_eq!(countered.objects.get(drake).counters.incubation, 0);
                assert!(!countered
                    .objects
                    .iter()
                    .any(|(_, o)| o.name == "Drake Token"));
                let mut reentered = branch.clone();
                event::propose_and_commit(
                    &mut reentered,
                    ProposedEvent::zone_change(drake, Zone::Hand),
                );
                event::propose_and_commit(
                    &mut reentered,
                    ProposedEvent::zone_change(drake, Zone::Battlefield),
                );
                finish(&mut reentered);
                custody::validate(&reentered).unwrap();
                assert_eq!(reentered.objects.get(drake).counters.incubation, 0);
                assert!(reentered
                    .objects
                    .iter()
                    .any(|(_, o)| o.zone == Zone::Battlefield
                        && o.name == "Drake Token"
                        && o.controller == actor));
                // Source departure never refunds costs and does not prevent the
                // independent token effect resolving for its original controller.
                event::propose_and_commit(branch, ProposedEvent::zone_change(drake, Zone::Hand));
                finish(branch);
                assert!(branch
                    .objects
                    .iter()
                    .any(|(_, o)| o.zone == Zone::Battlefield
                        && o.name == "Drake Token"
                        && o.controller == actor));
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    fn real_combat_assignment_block_prevention_suppression_and_strike_waves_both_seats() {
        use crate::card_def::Keywords;
        use crate::event::{ActiveReplacement, ReplacementEffectKind};
        use crate::state::Step;
        for actor in [PlayerId::P0, PlayerId::P1] {
            for mode in 0..6 {
                let (mut state, drake) = cast(actor);
                crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
                crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
                let blocker = put(
                    &mut state,
                    actor.opponent(),
                    "Llanowar Elves",
                    Zone::Battlefield,
                );
                if mode == 2 {
                    state.engine.active_replacements.push(ActiveReplacement {
                        id: 1,
                        source: drake,
                        kind: ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor.opponent()),
                            remaining: 10,
                        },
                    });
                }
                if mode == 3 {
                    let source = crate::state::ObjectLinkV4 {
                        object: blocker,
                        zone_change_count: state.objects.get(blocker).zone_change_count,
                    };
                    state
                        .objects
                        .get_mut(drake)
                        .v4
                        .creature_upgrade
                        .get_or_insert_with(Default::default)
                        .suppressed_by
                        .push(crate::standard_creatures_v1::TidebinderSuppressionV1 {
                            source,
                            timestamp: 1,
                        });
                    assert!(
                        !crate::continuous_characteristics_v1::printed_abilities_active(
                            &state, drake
                        )
                    );
                }
                if mode >= 4 {
                    state
                        .objects
                        .get_mut(drake)
                        .v4
                        .creature_upgrade
                        .get_or_insert_with(Default::default)
                        .keyword_grants
                        .push((
                            if mode == 4 {
                                Keywords::FIRST_STRIKE
                            } else {
                                Keywords::DOUBLE_STRIKE
                            },
                            1,
                        ));
                }
                let original_life = state.players[actor.opponent().index()].life;
                let mut attacked = false;
                let mut ended = false;
                for _ in 0..96 {
                    let decision = engine::advance_until_decision(&mut state);
                    if attacked && state.step == Step::EndCombat && state.stack.is_empty() {
                        ended = true;
                        break;
                    }
                    match decision {
                        Decision::CastSpellOrPass { .. } => {
                            engine::step(&mut state, Action::Pass).unwrap()
                        }
                        Decision::DeclareAttackers { .. } => {
                            engine::step(&mut state, Action::DeclareAttackers(vec![drake]))
                                .unwrap();
                            attacked = true;
                        }
                        Decision::DeclareBlockers { .. } => engine::step(
                            &mut state,
                            Action::DeclareBlockers(if mode == 1 {
                                vec![(blocker, drake)]
                            } else {
                                vec![]
                            }),
                        )
                        .unwrap(),
                        other => panic!("real Drake combat mode {mode}: {other:?}"),
                    }
                }
                assert!(ended, "combat mode {mode}");
                let expected = if mode == 1 || mode == 2 || mode == 3 {
                    0
                } else if mode == 5 {
                    2
                } else {
                    1
                };
                assert_eq!(state.objects.get(drake).counters.incubation, expected);
                if mode != 3 {
                    assert_eq!(
                        original_life - state.players[actor.opponent().index()].life,
                        expected
                    );
                }
                assert!(!state.objects.get(drake).tapped || mode == 3);
            }
        }
    }
    #[test]
    fn final_post_prevention_amount_is_saved_and_opponent_cannot_spend_it_both_seats() {
        use crate::event::{ActiveReplacement, ReplacementEffectKind};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            state.engine.active_replacements.push(ActiveReplacement {
                id: 1,
                source: drake,
                kind: ReplacementEffectKind::PreventNextDamage {
                    target: Target::Player(actor.opponent()),
                    remaining: 2,
                },
            });
            engine::commit_combat_damage_events(
                &mut state,
                vec![ProposedEvent::damage(
                    drake,
                    Target::Player(actor.opponent()),
                    5,
                )],
            );
            // The printed creature has power one, yet the saved event is three.
            finish(&mut state);
            assert_eq!(state.objects.get(drake).counters.incubation, 3);
            state.priority_player = actor.opponent();
            let before = state.clone();
            assert!(engine::step(&mut state, Action::ActivateAbility(drake, 0)).is_err());
            assert_eq!(
                state.objects.get(drake).counters.incubation,
                before.objects.get(drake).counters.incubation
            );
        }
    }
    #[test]
    fn only_original_capture_root_matches_printed_combat_binder() {
        let id = crate::card_def::card_id_by_name("Drake Hatcher").unwrap();
        let bound = EffectOp::DrakeIncubationFromCaptureV1 { capture: 9 };
        assert!(crate::trigger::trigger_effect_matches(id, &bound));
        assert!(!crate::trigger::trigger_effect_matches(
            crate::card_def::card_id_by_name("Forest").unwrap(),
            &bound
        ));
        // A numeric amount is description, never historical authority.
        for amount in [None, Some(-1), Some(0), Some(3), Some(i32::MAX)] {
            let effect = EffectOp::IncubationFromCombatDamageV1 { amount };
            if amount.is_some() {
                assert!(!crate::trigger::source_bound_trigger_program_matches(
                    &combat_binder(),
                    &effect
                ));
            }
            let (mut state, drake) = cast(PlayerId::P0);
            let mut ctx = ExecCtx::no_targets(drake, PlayerId::P0);
            ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, drake));
            crate::effect::execute(&effect, &ctx, &mut state);
            assert!(state.engine.halted.is_some());
            assert_eq!(state.objects.get(drake).counters.incubation, 0);
        }
    }
    #[test]
    fn exact_full_candidate_inventory_and_frozen_vector_refusal() {
        use crate::card_def::{card_id_by_name, CardCapability, CARD_DEFS};
        let id = card_id_by_name("Drake Hatcher").unwrap();
        assert_eq!(id, 448);
        assert_eq!(card_id_by_name("Drake Token"), Some(449));
        assert_eq!(CARD_DEFS.len(), 485);
        assert_eq!(CARD_DEFS[usize::from(id)].capability, CardCapability::Full);
        assert!(crate::card_def::preflight_fully_supported_deck(&[id]).is_ok());
        assert!(
            std::panic::catch_unwind(|| crate::rules_vector_v1::sources::card_rules(id)).is_err()
        );
        let empty = crate::state::Counters::default();
        assert!(serde_json::to_value(empty)
            .unwrap()
            .get("incubation")
            .is_none());
        let exact = crate::state::Counters {
            incubation: i32::MAX,
            ..empty
        };
        assert_eq!(serde_json::to_value(exact).unwrap()["incubation"], i32::MAX);
        assert!(exact.any());
        let back: crate::state::Counters =
            serde_json::from_slice(&serde_json::to_vec(&exact).unwrap()).unwrap();
        assert_eq!(back, exact);
    }
    #[test]
    fn source_incarnation_and_checked_arithmetic_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            let source = AbilitySourceContractV4::capture(&state, drake);
            let mut ctx = ExecCtx::no_targets(drake, actor);
            ctx.ability_source_contract = Some(source);
            assert!(!can_pay(&state, drake, SourceCounterKindV1::Incubation, 3));
            assert!(pay(&mut state, drake, SourceCounterKindV1::Incubation, 3).is_none());
            for invalid in [None, Some(0), Some(-1)] {
                assert!(place(&mut state, &ctx, invalid).is_err());
            }
            state.objects.get_mut(drake).counters.incubation = -1;
            assert!(place(&mut state, &ctx, Some(1)).is_err());
            assert!(!can_pay(&state, drake, SourceCounterKindV1::Incubation, 3));
            state.objects.get_mut(drake).counters.incubation = i32::MAX;
            let before = state.clone();
            assert!(place(&mut state, &ctx, Some(1)).is_err());
            assert_eq!(state, before);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(drake, Zone::Hand));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(drake, Zone::Battlefield),
            );
            assert_eq!(state.objects.get(drake).counters.incubation, 0);
            place(&mut state, &ctx, Some(3)).unwrap();
            assert_eq!(state.objects.get(drake).counters.incubation, 0);
        }
    }
    fn restored(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }
    fn reject_readonly(mut state: GameState) {
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    }
    fn damage_three(state: &mut GameState, drake: ObjectId, actor: PlayerId) {
        engine::commit_combat_damage_events(
            state,
            vec![ProposedEvent::damage(
                drake,
                Target::Player(actor.opponent()),
                3,
            )],
        );
    }
    #[test]
    fn original_damage_collection_publication_removal_and_payment_tamper_both_seats() {
        use custody::{journal, JournalV1 as J};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            damage_three(&mut state, drake, actor);
            let _ = engine::advance_until_decision(&mut state);
            custody::validate(&state).unwrap();
            let original = restored(&state);
            // Genuine collected and published capture survives owning restore.
            let capture = state
                .engine
                .event_history
                .iter()
                .filter_map(journal)
                .find_map(|r| {
                    if let J::Captured { amount, source, .. } = &r.value {
                        assert_eq!(*amount, 3);
                        assert_eq!(source.controller, actor);
                        Some(r.index)
                    } else {
                        None
                    }
                })
                .unwrap();
            for mode in 0..8 {
                let mut bad = original.clone();
                match mode {
                    0 => bad.engine.event_history.retain(|e| {
                        journal(e).is_none_or(|r| !matches!(r.value, J::Captured { .. }))
                    }),
                    1 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .find_map(|e| {
                                if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                    matches!(record.value, J::Captured { .. }).then_some(record)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        if let J::Captured { amount, .. } = &mut row.value {
                            *amount = 1;
                        }
                    }
                    2 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .find_map(|e| {
                                if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                    matches!(record.value, J::Captured { .. }).then_some(record)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        if let J::Captured { source, .. } = &mut row.value {
                            source.controller = actor.opponent();
                        }
                    }
                    3 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .find_map(|e| {
                                if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                    matches!(record.value, J::Captured { .. }).then_some(record)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        if let J::Captured { source, .. } = &mut row.value {
                            source.zone_change_count += 1;
                        }
                    }
                    4 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter()
                            .find(|e| {
                                journal(e).is_some_and(|r| matches!(r.value, J::Captured { .. }))
                            })
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(row);
                        bad.engine.drake_ledger_v1.issued += 1;
                    }
                    5 => bad.engine.event_history.retain(|e| {
                        journal(e).is_none_or(|r| !matches!(r.value, J::Collected { .. }))
                    }),
                    6 => bad.engine.event_history.retain(|e| {
                        journal(e).is_none_or(|r| !matches!(r.value, J::Enclosing { .. }))
                    }),
                    _ => {
                        bad.stack.last_mut().unwrap().inline_effect =
                            Some(EffectOp::DrakeIncubationFromCaptureV1 {
                                capture: capture + 1,
                            })
                    }
                }
                reject_readonly(restored(&bad));
            }
            finish(&mut state);
            assert_eq!(state.objects.get(drake).counters.incubation, 3);
            custody::validate(&state).unwrap();
            state.priority_player = actor;
            engine::step(&mut state, Action::ActivateAbility(drake, 0)).unwrap();
            let _ = engine::advance_until_decision(&mut state);
            assert_eq!(state.objects.get(drake).counters.incubation, 0);
            custody::validate(&state).unwrap();
            for mode in 0..6 {
                let mut bad = restored(&state);
                match mode {
                    0 => bad
                        .engine
                        .event_history
                        .retain(|e| journal(e).is_none_or(|r| !matches!(r.value, J::Debit { .. }))),
                    1 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .rev()
                            .find_map(|e| {
                                if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                    matches!(record.value, J::Debit { .. }).then_some(record)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        if let J::Debit { count, .. } = &mut row.value {
                            *count = 2;
                        }
                    }
                    2 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter_mut()
                            .rev()
                            .find_map(|e| {
                                if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                    matches!(record.value, J::Paid { .. }).then_some(record)
                                } else {
                                    None
                                }
                            })
                            .unwrap();
                        if let J::Paid { original, .. } = &mut row.value {
                            original.objects.get_mut(drake).counters.incubation += 1;
                        }
                    }
                    3 => bad
                        .engine
                        .event_history
                        .retain(|e| journal(e).is_none_or(|r| !matches!(r.value, J::Paid { .. }))),
                    4 => {
                        let row = bad
                            .engine
                            .event_history
                            .iter()
                            .rev()
                            .find(|e| journal(e).is_some_and(|r| matches!(r.value, J::Paid { .. })))
                            .unwrap()
                            .clone();
                        bad.engine.event_history.push(row);
                        bad.engine.drake_ledger_v1.issued += 1;
                    }
                    _ => bad.stack.last_mut().unwrap().controller = actor.opponent(),
                }
                reject_readonly(bad);
            }
            let paid = state.clone();
            let id = state.stack.last().unwrap().v4.stack_item_id;
            let mut countered = restored(&state);
            engine::counter_stack_item_by_id(&mut countered, id).unwrap();
            custody::validate(&countered).unwrap();
            assert_eq!(countered.objects.get(drake).counters.incubation, 0);
            let item = paid.stack.last().unwrap().clone();
            countered.stack.push(item);
            reject_readonly(countered);
            finish(&mut state);
            custody::validate(&state).unwrap();
            let mut erased = restored(&state);
            erased
                .engine
                .event_history
                .retain(|e| journal(e).is_none_or(|r| !matches!(r.value, J::Completed { .. })));
            reject_readonly(erased);
            assert!(state
                .objects
                .iter()
                .any(|(_, o)| o.name == "Drake Token" && o.controller == actor));
        }
    }
    #[test]
    fn genuine_apnap_capture_ownership_and_leave_reenter_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            let own = put(&mut state, actor, "Drake Hatcher", Zone::Battlefield);
            let other = put(
                &mut state,
                actor.opponent(),
                "Drake Hatcher",
                Zone::Battlefield,
            );
            engine::commit_combat_damage_events(
                &mut state,
                vec![
                    ProposedEvent::damage(drake, Target::Player(actor.opponent()), 3),
                    ProposedEvent::damage(own, Target::Player(actor.opponent()), 1),
                    ProposedEvent::damage(other, Target::Player(actor), 2),
                ],
            );
            let state = restored(&state);
            custody::validate(&state).unwrap();
            for mode in 0..2 {
                let mut branch = state.clone();
                if mode == 0 {
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(drake, Zone::Hand),
                    );
                    event::propose_and_commit(
                        &mut branch,
                        ProposedEvent::zone_change(drake, Zone::Battlefield),
                    );
                } else {
                    // Intervening control fixture preserves the actual battlefield
                    // roster as well as the object's current controller.
                    branch.players[actor.index()]
                        .battlefield
                        .retain(|id| *id != drake);
                    branch.players[actor.opponent().index()]
                        .battlefield
                        .push(drake);
                    branch.objects.get_mut(drake).controller = actor.opponent();
                }
                finish(&mut branch);
                custody::validate(&branch).unwrap();
                assert_eq!(
                    branch.objects.get(drake).counters.incubation,
                    if mode == 0 { 0 } else { 3 }
                );
                assert_eq!(branch.objects.get(own).counters.incubation, 1);
                assert_eq!(branch.objects.get(other).counters.incubation, 2);
            }
        }
    }
    #[test]
    fn genuine_banner_checkpoint_nested_original_restore_erasure_and_shift_both_seats() {
        use custody::{journal, JournalV1 as J};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            damage_three(&mut state, drake, actor);
            finish(&mut state);
            state.priority_player = actor;
            let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            engine::step(&mut state, Action::CastSpell(banner)).unwrap();
            let mut reached = false;
            for _ in 0..24 {
                match engine::advance_until_decision(&mut state) {
                    Decision::ChooseEntryColorV36 { player, .. } => {
                        assert_eq!(player, actor);
                        reached = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("genuine Drake/Banner checkpoint: {other:?}"),
                }
            }
            assert!(reached);
            let mut original = restored(&state);
            custody::validate(&original).unwrap();
            for mode in 0..4 {
                let mut bad = original.clone();
                if mode == 3 {
                    // Keep index/position unchanged but alter the whole nested
                    // record. Compression must not erase this disagreement.
                    let nested = &mut bad
                        .engine
                        .linked_aura_operation_v1
                        .pending_general_v36
                        .as_mut()
                        .unwrap()
                        .original
                        .engine
                        .event_history;
                    let captured = nested
                        .iter_mut()
                        .find_map(|event| match event {
                            event::CommittedEvent::DrakeJournalV1 { record }
                                if matches!(record.value, J::Captured { .. }) =>
                            {
                                Some(record)
                            }
                            _ => None,
                        })
                        .unwrap();
                    let J::Captured { amount, .. } = &mut captured.value else {
                        unreachable!()
                    };
                    *amount += 1;
                    assert!(custody::validate(&bad).is_err());
                } else if mode < 2 {
                    let nested = &mut bad
                        .engine
                        .linked_aura_operation_v1
                        .pending_general_v36
                        .as_mut()
                        .unwrap()
                        .original
                        .engine
                        .event_history;
                    let position = nested.iter().position(|e| journal(e).is_some()).unwrap();
                    if mode == 0 {
                        nested.remove(position);
                    } else {
                        nested.swap(position, position + 1);
                    }
                } else {
                    // Refuse erased original references inside the saved complete
                    // sibling output, even when the live checkpoint is unchanged.
                    let row = bad
                        .engine
                        .event_history
                        .iter_mut()
                        .rev()
                        .find_map(|e| {
                            if let event::CommittedEvent::DrakeJournalV1 { record } = e {
                                matches!(record.value, J::Enclosing { .. }).then_some(record)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    let J::Enclosing { output, .. } = &mut row.value else {
                        unreachable!()
                    };
                    let nested = &mut output
                        .engine
                        .linked_aura_operation_v1
                        .pending_general_v36
                        .as_mut()
                        .unwrap()
                        .original
                        .engine
                        .event_history;
                    let position = nested
                        .iter()
                        .position(|e| matches!(e, event::CommittedEvent::DrakeHistoryRefV1 { .. }))
                        .unwrap();
                    nested.remove(position);
                }
                let mut bad = restored(&bad);
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(engine::step(&mut bad, Action::ChooseEffectOption(1)).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
            for branch in [&mut state, &mut original] {
                engine::step(branch, Action::ChooseEffectOption(1)).unwrap();
                finish(branch);
                custody::validate(branch).unwrap();
                assert_eq!(branch.objects.get(banner).zone, Zone::Battlefield);
                assert_eq!(branch.objects.get(drake).counters.incubation, 3);
            }
            assert_eq!(state, original);
        }
    }
    #[test]
    fn absent_owner_keeps_zero_ledger_json_hash_and_ordinary_steps() {
        use std::hash::{Hash, Hasher};
        #[derive(Default)]
        struct Bytes(Vec<u8>);
        impl Hasher for Bytes {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let mut bytes = Bytes::default();
        custody::LedgerV1::default().hash(&mut bytes);
        assert!(bytes.0.is_empty());
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let land = put(&mut state, actor, "Island", Zone::Hand);
            let mut old = state.clone();
            engine::step(&mut state, Action::PlayLand(land)).unwrap();
            engine::step_without_linked_aura_v1(&mut old, Action::PlayLand(land)).unwrap();
            assert_eq!(state, old);
            assert!(state.engine.drake_ledger_v1.is_empty());
            assert!(serde_json::to_value(&state.engine)
                .unwrap()
                .get("drake_ledger_v1")
                .is_none());
            assert!(state
                .engine
                .event_history
                .iter()
                .all(|e| custody::journal(e).is_none()));
        }
    }
    #[test]
    fn genuine_longbow_granted_paid_slot_remains_independent_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, drake) = cast(actor);
            crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, actor);
            let bow = put(&mut state, actor, "Viridian Longbow", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
            state.priority_player = actor;
            engine::step(&mut state, Action::CastSpell(bow)).unwrap();
            finish(&mut state);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            state.priority_player = actor;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            engine::step(&mut state, Action::ActivateAbility(bow, 0)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(drake))).unwrap();
            finish(&mut state);
            assert_eq!(state.objects.get(bow).v4.attached_to.unwrap().object, drake);
            assert_eq!(
                state.players[actor.index()].mana_pool[ManaColor::C.pool_index()],
                0
            );
            let mut own_restore = restored(&state);
            for branch in [&mut state, &mut own_restore] {
                branch.priority_player = actor;
                assert!(!branch.objects.get(drake).summoning_sick);
                let life = branch.players[actor.opponent().index()].life;
                engine::step(branch, Action::ActivateAbility(drake, 1)).unwrap();
                assert!(matches!(
                    engine::advance_until_decision(branch),
                    Decision::ChooseTargets { .. }
                ));
                engine::step(
                    branch,
                    Action::ChooseTarget(Target::Player(actor.opponent())),
                )
                .unwrap();
                let _ = engine::advance_until_decision(branch);
                assert!(branch.objects.get(drake).tapped);
                assert_eq!(branch.objects.get(drake).counters.incubation, 0);
                assert!(branch.stack.last().unwrap().v4.granted_by.is_some());
                finish(branch);
                custody::validate(branch).unwrap();
                assert_eq!(branch.players[actor.opponent().index()].life, life - 1);
                assert!(!branch
                    .engine
                    .event_history
                    .iter()
                    .filter_map(custody::journal)
                    .any(|row| matches!(row.value, custody::JournalV1::Paid { .. })));
            }
            assert_eq!(state, own_restore);
        }
    }
    fn paid_replay_memo_state(actor: PlayerId) -> (GameState, ObjectId) {
        let (mut state, drake) = cast(actor);
        // This ordinary paid cast occurs while the public Hatcher is already
        // active, so its actual SpellCast event is inside an enclosing Step.
        let ordinary = put(&mut state, actor, "Think Twice", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        state.priority_player = actor;
        engine::step(&mut state, Action::CastSpell(ordinary)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor
        ));
        assert!(state.engine.pending_cast.is_none());
        let row = state
            .stack
            .iter()
            .find(|row| row.source == ordinary)
            .unwrap();
        assert_eq!(row.kind, crate::state::StackItemKind::Spell);
        assert_eq!(row.controller, actor);
        assert!(row.targets.is_empty() && row.v4.target_contracts.is_empty());
        assert_eq!(state.objects.get(ordinary).zone, Zone::Stack);
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        finish(&mut state);
        assert_eq!(state.objects.get(ordinary).zone, Zone::Graveyard);
        damage_three(&mut state, drake, actor);
        finish(&mut state);
        assert_eq!(state.objects.get(drake).counters.incubation, 3);
        state.priority_player = actor;
        engine::step(&mut state, Action::ActivateAbility(drake, 0)).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        assert_eq!(state.objects.get(drake).counters.incubation, 0);
        assert_eq!(
            state.stack.last().unwrap().kind,
            crate::state::StackItemKind::ActivatedAbility
        );
        custody::validate(&state).unwrap();
        (state, ordinary)
    }

    #[test]
    fn genuine_paid_enclosing_replay_memo_warm_restore_and_ordinary_segment_tamper_both_seats() {
        use custody::{journal, JournalV1 as J};
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, paid_spell) = paid_replay_memo_state(actor);
            let original = state.clone();
            custody::reset_replay_memo_for_test();
            custody::validate(&state).unwrap();
            let cold = custody::replay_memo_stats_for_test();
            assert!(cold.executions > 0);
            assert_eq!(cold.hits, 0);
            assert_eq!(cold.ineligible, 0);
            custody::validate(&state).unwrap();
            let warm = custody::replay_memo_stats_for_test();
            assert_eq!(warm.executions, cold.executions);
            assert!(warm.hits > cold.hits);
            assert_eq!(state, original);
            let restored_state = restored(&state);
            custody::validate(&restored_state).unwrap();
            let after_restore = custody::replay_memo_stats_for_test();
            assert_eq!(after_restore.executions, warm.executions);
            assert!(after_restore.hits > warm.hits);
            assert_eq!(restored_state, state);

            let ordinary = state.engine.event_history.iter().position(|event| {
                    matches!(event, event::CommittedEvent::SpellCast { spell, .. } if *spell == paid_spell)
            }).unwrap();
            let (start, end) = state
                .engine
                .event_history
                .iter()
                .filter_map(journal)
                .find_map(|row| {
                    if let J::Enclosing { start, .. } = row.value {
                        let end = usize::try_from(row.index).unwrap();
                        let start = usize::try_from(start).unwrap();
                        (start <= ordinary && ordinary < end).then_some((start, end))
                    } else {
                        None
                    }
                })
                .expect("genuine cast event lies inside an actual enclosing segment");
            let mut bad = state.clone();
            let event::CommittedEvent::SpellCast { controller, .. } =
                &mut bad.engine.event_history[ordinary]
            else {
                unreachable!()
            };
            *controller = actor.opponent();
            // The saved row and input prefix stay identical. Only its actual
            // ordinary-event segment changes, which the memo key must bind.
            assert_eq!(
                bad.engine.event_history[..start],
                state.engine.event_history[..start]
            );
            assert_eq!(
                bad.engine.event_history[end],
                state.engine.event_history[end]
            );
            let bytes = serde_json::to_vec(&bad).unwrap();
            for _ in 0..2 {
                let before = custody::replay_memo_stats_for_test();
                assert_eq!(
                    custody::validate(&bad).unwrap_err(),
                    "Drake complete enclosing original operation/history replay differs"
                );
                let after = custody::replay_memo_stats_for_test();
                assert!(after.executions > before.executions);
                assert_eq!(after.ineligible, before.ineligible);
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
            let before = custody::replay_memo_stats_for_test();
            custody::validate(&state).unwrap();
            assert_eq!(
                custody::replay_memo_stats_for_test().executions,
                before.executions
            );
            assert_eq!(state, original);
        }
    }

    fn last_replay_output(state: &mut GameState) -> &mut GameState {
        state
            .engine
            .event_history
            .iter_mut()
            .rev()
            .find_map(|event| {
                if let event::CommittedEvent::DrakeJournalV1 { record } = event {
                    if let custody::JournalV1::Enclosing { output, .. } = &mut record.value {
                        return Some(output.as_mut());
                    }
                }
                None
            })
            .unwrap()
    }

    #[test]
    fn genuine_paid_observation_key_frame_keeps_foreign_tamper_restore_and_borrow_boundaries_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _) = paid_replay_memo_state(actor);
            custody::validate(&state).unwrap();
            custody::validate(&state).unwrap();
            let bytes = serde_json::to_vec(&state).unwrap();
            let foreign = restored(&state);
            assert_eq!(foreign, state);
            custody::reset_replay_key_frame_stats_for_test();
            custody::with_observation_replay_keys(&state, |borrowed| {
                custody::validate(borrowed).unwrap();
                assert_eq!(custody::replay_key_frame_stats_for_test(), (1, 0));
                custody::validate(borrowed).unwrap();
                assert_eq!(custody::replay_key_frame_stats_for_test(), (1, 1));

                // Equal restored worlds have distinct live allocations. They
                // must build their own exact typed-history keys.
                custody::validate(&foreign).unwrap();
                assert_eq!(custody::replay_key_frame_stats_for_test(), (2, 1));
                custody::with_observation_replay_keys(&foreign, |nested| {
                    custody::validate(nested).unwrap();
                    custody::validate(nested).unwrap();
                });
                assert_eq!(custody::replay_key_frame_stats_for_test(), (3, 2));
                custody::validate(borrowed).unwrap();
                assert_eq!(custody::replay_key_frame_stats_for_test(), (3, 3));

                let mut changed = borrowed.clone();
                last_replay_output(&mut changed).players[actor.index()].life += 1;
                let changed_bytes = serde_json::to_vec(&changed).unwrap();
                assert_eq!(
                    custody::validate(&changed).unwrap_err(),
                    "Drake complete enclosing original operation/history replay differs"
                );
                assert!(custody::replay_key_frame_stats_for_test().0 > 3);
                assert_eq!(serde_json::to_vec(&changed).unwrap(), changed_bytes);

                let mut private = borrowed.clone();
                crate::linked_aura_operation_v1::tamper_drake_saved_output_runtime_for_test(
                    last_replay_output(&mut private),
                );
                assert_eq!(serde_json::to_vec(&private).unwrap(), bytes);
                assert_ne!(private, *borrowed);
                let original_private = private.clone();
                assert_eq!(
                    custody::validate(&private).unwrap_err(),
                    "Drake complete enclosing original operation/history replay differs"
                );
                assert_eq!(private, original_private);
                let before = custody::replay_key_frame_stats_for_test();
                custody::validate(borrowed).unwrap();
                let after = custody::replay_key_frame_stats_for_test();
                assert_eq!(after, (before.0, before.1 + 1));
            });
            let before = custody::replay_key_frame_stats_for_test();
            custody::validate(&state).unwrap();
            assert_eq!(
                custody::replay_key_frame_stats_for_test(),
                (before.0 + 1, before.1)
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);

            // Unwinding an inner scope restores its predecessor; unwinding the
            // outer scope removes all keys before a later validation call.
            let stopped = std::panic::catch_unwind(|| {
                custody::with_observation_replay_keys(&state, |borrowed| {
                    custody::validate(borrowed).unwrap();
                    panic!("intentional observation-frame unwind");
                });
            });
            assert!(stopped.is_err());
            let before = custody::replay_key_frame_stats_for_test();
            custody::validate(&state).unwrap();
            assert_eq!(
                custody::replay_key_frame_stats_for_test(),
                (before.0 + 1, before.1)
            );
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        }
    }

    #[test]
    fn genuine_paid_enclosing_replay_memo_changed_output_and_private_runtime_fallback_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _) = paid_replay_memo_state(actor);
            custody::reset_replay_memo_for_test();
            custody::validate(&state).unwrap();
            custody::validate(&state).unwrap();
            let warm = custody::replay_memo_stats_for_test();
            assert!(warm.hits > 0);
            let mut changed = state.clone();
            last_replay_output(&mut changed).players[actor.index()].life += 1;
            assert_eq!(
                custody::validate(&changed).unwrap_err(),
                "Drake complete enclosing original operation/history replay differs"
            );
            let after_changed = custody::replay_memo_stats_for_test();
            assert!(after_changed.executions > warm.executions);
            assert_eq!(after_changed.ineligible, warm.ineligible);

            let mut private = state.clone();
            crate::linked_aura_operation_v1::tamper_drake_saved_output_runtime_for_test(
                last_replay_output(&mut private),
            );
            // Serialized commitments alone cannot distinguish these typed
            // states. Round-trip eligibility must force the existing replay.
            assert_eq!(
                serde_json::to_vec(&private).unwrap(),
                serde_json::to_vec(&state).unwrap()
            );
            assert_ne!(private, state);
            assert_eq!(restored(&private), state);
            let before_private = custody::replay_memo_stats_for_test();
            let before_typed = private.clone();
            assert_eq!(
                custody::validate(&private).unwrap_err(),
                "Drake complete enclosing original operation/history replay differs"
            );
            let after_private = custody::replay_memo_stats_for_test();
            assert!(after_private.executions > before_private.executions);
            assert!(after_private.ineligible > before_private.ineligible);
            assert_eq!(private, before_typed);
            let before_good = custody::replay_memo_stats_for_test();
            custody::validate(&restored(&private)).unwrap();
            assert_eq!(
                custody::replay_memo_stats_for_test().executions,
                before_good.executions
            );
        }
    }

    #[test]
    fn genuine_paid_replay_prefix_keys_match_original_after_restore_append_change_and_truncation_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, ordinary) = paid_replay_memo_state(actor);
            let original = state.clone();
            let history = &state.engine.event_history;
            let oracle = custody::original_replay_keys_for_test(history);
            assert!(!oracle.is_empty());
            custody::clear_replay_prefix_for_test();
            let (cold, retained, bytes, reused) = custody::replay_prefix_probe_for_test(history);
            assert_eq!(cold, oracle);
            assert_eq!(reused, 0);
            assert!(retained > 0 && retained <= 512);
            assert!(bytes > 0 && bytes <= 64 * 1024 * 1024);
            let restored = restored(&state);
            let (warm, _, _, reused) =
                custody::replay_prefix_probe_for_test(&restored.engine.event_history);
            assert_eq!(warm, oracle);
            assert_eq!(reused, retained);
            custody::validate(&restored).unwrap();

            let position = history.iter().position(|event| {
                matches!(event, event::CommittedEvent::SpellCast { spell, .. } if *spell == ordinary)
            }).unwrap();
            let mut changed = original.clone();
            let event::CommittedEvent::SpellCast { controller, .. } =
                &mut changed.engine.event_history[position]
            else {
                unreachable!()
            };
            *controller = actor.opponent();
            // Prime the original prefix immediately before the changed request.
            custody::replay_prefix_probe_for_test(history);
            let (keys, _, _, reused) =
                custody::replay_prefix_probe_for_test(&changed.engine.event_history);
            assert_eq!(reused, position.min(retained));
            assert_eq!(
                keys,
                custody::original_replay_keys_for_test(&changed.engine.event_history)
            );
            assert_ne!(keys, oracle);
            let before = changed.clone();
            assert!(custody::validate(&changed).is_err());
            assert_eq!(changed, before);

            // Truncation is a pure key-input probe, not a fabricated native
            // history claimed to pass custody validation.
            custody::replay_prefix_probe_for_test(history);
            let shortened = &history[..position];
            let (keys, rows, _, reused) = custody::replay_prefix_probe_for_test(shortened);
            assert_eq!(keys, custody::original_replay_keys_for_test(shortened));
            assert_eq!(rows, position.min(retained));
            assert_eq!(reused, rows);
            assert_eq!(state, original);

            // Complete the already genuinely paid activation through the native
            // driver. Its real appended history must retain every original key.
            custody::replay_prefix_probe_for_test(history);
            finish(&mut state);
            custody::validate(&state).unwrap();
            let (keys, _, _, _) =
                custody::replay_prefix_probe_for_test(&state.engine.event_history);
            assert_eq!(
                keys,
                custody::original_replay_keys_for_test(&state.engine.event_history)
            );
            assert!(oracle
                .iter()
                .all(|(position, key)| keys.get(position) == Some(key)));
        }
    }

    #[test]
    fn genuine_paid_replay_prefix_rejects_same_serde_nested_runtime_and_limits_retention_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, ordinary) = paid_replay_memo_state(actor);
            custody::clear_replay_prefix_for_test();
            let (keys, retained, _, _) =
                custody::replay_prefix_probe_for_test(&state.engine.event_history);
            let nested = state
                .engine
                .event_history
                .iter()
                .take(retained)
                .position(|event| {
                    matches!(
                        custody::journal(event).map(|row| &row.value),
                        Some(custody::JournalV1::Enclosing { .. })
                    )
                })
                .expect("a genuine enclosing row is retained before the budget");
            let mut private = state.clone();
            let event::CommittedEvent::DrakeJournalV1 { record } =
                &mut private.engine.event_history[nested]
            else {
                unreachable!()
            };
            let custody::JournalV1::Enclosing { output, .. } = &mut record.value else {
                unreachable!()
            };
            crate::linked_aura_operation_v1::tamper_drake_saved_output_runtime_for_test(output);
            assert_eq!(
                serde_json::to_vec(&private).unwrap(),
                serde_json::to_vec(&state).unwrap()
            );
            assert_ne!(private, state);
            let before = private.clone();
            let (ineligible, rows, _, reused) =
                custody::replay_prefix_probe_for_test(&private.engine.event_history);
            assert_eq!(reused, nested);
            assert_eq!(rows, nested);
            assert_eq!(
                ineligible,
                custody::original_replay_keys_for_test(&private.engine.event_history)
            );
            assert!(ineligible.keys().all(|position| *position < nested as u64));
            assert_ne!(ineligible, keys);
            assert!(custody::validate(&private).is_err());
            assert_eq!(private, before);
            custody::validate(&state).unwrap();
            assert_eq!(
                custody::replay_prefix_probe_for_test(&state.engine.event_history).0,
                keys
            );

            // Repeated actual typed ordinary events isolate the count budget;
            // this input is a key-computation primitive, not native play.
            let ordinary = state.engine.event_history.iter().find(|event| {
                matches!(event, event::CommittedEvent::SpellCast { spell, .. } if *spell == ordinary)
            }).unwrap();
            let primitive = vec![ordinary.clone(); 513];
            custody::clear_replay_prefix_for_test();
            let (keys, rows, bytes, _) = custody::replay_prefix_probe_for_test(&primitive);
            assert_eq!(keys, custody::original_replay_keys_for_test(&primitive));
            assert_eq!(rows, 512);
            assert!(bytes <= 64 * 1024 * 1024);
            let (_, rows, _, reused) = custody::replay_prefix_probe_for_test(&primitive);
            assert_eq!((rows, reused), (512, 512));
        }
    }

    #[test]
    fn fresh_concealed_printing_never_selects_drake_owner_or_refusal_both_seats() {
        use crate::state::FaceDownV1;
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut worlds = Vec::new();
            for name in ["Drake Hatcher", "Llanowar Elves"] {
                // Primitive worlds isolate concealment, not an admitted
                // disguise producer for either printed creature.
                let mut state = ready_state_for(actor);
                let source = put(&mut state, actor, name, Zone::Battlefield);
                state.objects.get_mut(source).v4.face_down_v1 = Some(FaceDownV1 {
                    disguised: true,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
                assert!(!custody::relevant(&state));
                assert!(require_public_counter(&state, source).is_ok());
                engine::step(&mut state, Action::Pass).unwrap();
                assert!(state.engine.drake_ledger_v1.is_empty());
                engine::commit_combat_damage_events(
                    &mut state,
                    vec![ProposedEvent::damage(
                        source,
                        Target::Player(actor.opponent()),
                        1,
                    )],
                );
                assert!(state.engine.halted.is_none());
                assert!(state.engine.drake_ledger_v1.is_empty());
                assert_eq!(state.objects.get(source).counters.incubation, 0);
                assert_eq!(state.players[actor.opponent().index()].life, 19);
                assert!(!state
                    .engine
                    .event_history
                    .iter()
                    .any(|event| custody::journal(event).is_some()));
                let predecessor = crate::rl::observe_v1(&state, actor, 0)
                    .ok()
                    .map(|view| serde_json::to_vec(&view).unwrap());
                worlds.push((require_public_counter(&state, source).is_ok(), predecessor));
            }
            assert_eq!(worlds[0], worlds[1]);
        }
    }
}
