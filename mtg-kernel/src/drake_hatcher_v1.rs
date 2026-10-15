//! Genuine Partial Drake Hatcher source. Consumer qualification is deferred.
use crate::{
    effect::{EffectOp, ExecCtx},
    ids::ObjectId,
    state::{GameState, Zone},
};
use serde::{Deserialize, Serialize};

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
    let after = count(state, source, kind)?.checked_sub(i32::from(amount))?;
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
    EffectOp::CreateToken {
        token_def: crate::card_def::card_id_by_name("Drake Token").expect("genuine Drake token"),
        count: 1,
    }
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
    if state.objects.try_get(id).is_some_and(|o| {
        o.counters.incubation != 0
            || (o.v4.face_down_v1.is_none()
                && crate::card_def::CARD_DEFS
                    .get(usize::from(o.card_def))
                    .is_some_and(|d| d.name == "Drake Hatcher"))
    }) {
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
    fn exact_partial_inventory_and_frozen_vector_refusal() {
        use crate::card_def::{card_id_by_name, CardCapability, CARD_DEFS};
        let id = card_id_by_name("Drake Hatcher").unwrap();
        assert_eq!(id, 444);
        assert_eq!(card_id_by_name("Drake Token"), Some(445));
        assert_eq!(CARD_DEFS.len(), 446);
        assert_eq!(
            CARD_DEFS[usize::from(id)].capability,
            CardCapability::Partial
        );
        assert!(crate::card_def::preflight_fully_supported_deck(&[id]).is_err());
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
}
