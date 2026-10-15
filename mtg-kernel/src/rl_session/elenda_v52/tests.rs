//! Genuine casts and choices through the sealed owner. Initial life, mana and
//! High Fae battlefield support are declared fixture inputs, not paid outcomes.
use super::*;
use crate::{ids::ObjectId, policy_observation_v6::tests::put, rl::TargetRefV1, state::Zone};
const ELENDA: &str = "Elenda, Saint of Dusk";

fn raw(actor: PlayerId, life: i32) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    state.players[actor.index()].life = life;
    for player in [actor, actor.opponent()] {
        state.players[player.index()].mana_pool = [10; 6];
    }
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    state
}
fn own(state: GameState) -> ElendaSessionV52 {
    ElendaSessionV52::from_session(FastActorSessionV1::from_elenda_v52_fixture_state(state))
        .unwrap()
}
fn act(session: &mut ElendaSessionV52, choose: impl Fn(&ActionSemanticV1) -> bool) {
    let decision = session.decision_v52().unwrap();
    let index = decision
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("actual V52 choice absent: {:?}", decision.actions()));
    session.step_v52(&decision, index as u32).unwrap();
}
fn priority(session: &mut ElendaSessionV52, actor: PlayerId) {
    for _ in 0..8 {
        if session.decision_v52().unwrap().expected().acting_player == actor.into() {
            return;
        }
        act(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("priority absent");
}
fn cast(
    session: &mut ElendaSessionV52,
    actor: PlayerId,
    spell: ObjectId,
    target: Option<ObjectId>,
) {
    priority(session, actor);
    let before: u32 = session.inner.state.players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    let value = u32::from(
        crate::card_def::CARD_DEFS[usize::from(session.inner.state.objects.get(spell).card_def)]
            .mana_value,
    );
    act(
        session,
        |a| matches!(a, ActionSemanticV1::CastSpell { source, .. } if source.arena_id == spell.0),
    );
    if let Some(target) = target {
        act(
            session,
            |a| matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0),
        );
    }
    let after: u32 = session.inner.state.players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    assert_eq!(before - after, value, "genuine printed payment");
    assert!(session
        .inner
        .state
        .stack
        .iter()
        .any(|item| item.source == spell
            && item.v4.cast_method == Some(crate::state::CastMethodV4::Normal)));
}
fn drain_to(session: &mut ElendaSessionV52, remaining: usize) -> ElendaObservationV52 {
    for _ in 0..64 {
        let decision = session.decision_v52().unwrap();
        if session.inner.state.stack.len() == remaining
            && session.inner.state.engine.pending_triggers.is_empty()
        {
            return decision.observation().clone();
        }
        assert!(session.inner.state.stack.len() >= remaining);
        act(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("V52 resolution did not finish");
}
fn row(public: &ElendaObservationV52, id: ObjectId) -> &crate::elenda_public_v52::CurrentV52 {
    public
        .elenda
        .current
        .iter()
        .find(|row| row.source.arena_id == id.0)
        .unwrap()
}
fn gameplay(session: &ElendaSessionV52) -> (Vec<u8>, u64, u64) {
    (
        serde_json::to_vec(session.inner.elenda_fixture_state()).unwrap(),
        session.inner.policy_step_count(),
        session.inner.physical_decision_count(),
    )
}
fn assert_stats(public: &ElendaObservationV52, id: ObjectId, pt: i32, menace: bool) {
    let row = row(public, id);
    assert_eq!(
        (row.effective_power, row.effective_toughness),
        (Some(pt), Some(pt))
    );
    assert_eq!(row.effective_menace, menace);
    assert!(
        row.effective_lifelink
            && row.hexproof_from_opponents_instants
            && row.printed_abilities_active
    );
    assert_eq!(
        row.ordered_bonuses
            .iter()
            .map(|bonus| bonus.minimum_above_start)
            .collect::<Vec<_>>(),
        [1, 10]
    );
}
#[test]
fn genuine_paid_thresholds_downward_transitions_and_owning_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (life, power, menace) in [(20, 4, false), (21, 5, true), (29, 5, true), (30, 10, true)]
        {
            let mut state = raw(actor, life);
            let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
            let bolts = std::array::from_fn::<_, 4, _>(|_| {
                put(&mut state, actor, "Lightning Bolt", Zone::Hand)
            });
            let mut session = own(state);
            cast(&mut session, actor, elenda, None);
            let paid = session.snapshot_v52();
            let stale = session.decision_v52().unwrap();
            let public = drain_to(&mut session, 0);
            assert_stats(&public, elenda, power, menace);
            let first = gameplay(&session);
            session.restore_v52(&paid).unwrap();
            let before = gameplay(&session);
            assert!(session.step_v52(&stale, 0).is_err());
            assert_eq!(gameplay(&session), before);
            assert_eq!(drain_to(&mut session, 0), public);
            assert_eq!(gameplay(&session), first);
            // Actual self-targeted three-damage spells cross both thresholds.
            if life == 30 {
                for bolt in bolts {
                    priority(&mut session, actor);
                    let red_before = session.inner.state.players[actor.index()].mana_pool
                        [crate::mana::ManaColor::R.pool_index()];
                    act(
                        &mut session,
                        |a| matches!(a, ActionSemanticV1::CastSpell { source, .. } if source.arena_id == bolt.0),
                    );
                    act(
                        &mut session,
                        |a| matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } if *player == actor.into()),
                    );
                    assert_eq!(
                        session.inner.state.players[actor.index()].mana_pool
                            [crate::mana::ManaColor::R.pool_index()],
                        red_before - 1
                    );
                    let public = drain_to(&mut session, 0);
                    let life = session.inner.state.players[actor.index()].life;
                    assert_stats(&public, elenda, if life > 20 { 5 } else { 4 }, life > 20);
                }
                assert_stats(
                    &session.decision_v52().unwrap().observation,
                    elenda,
                    4,
                    false,
                );
            }
        }
    }
}
#[test]
fn genuine_paid_control_suppression_and_pending_target_revalidation_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let employment = put(
            &mut state,
            actor.opponent(),
            "Involuntary Employment",
            Zone::Hand,
        );
        let witness = put(
            &mut state,
            actor.opponent(),
            "Witness Protection",
            Zone::Hand,
        );
        put(
            &mut state,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let mut session = own(state);
        cast(&mut session, actor, elenda, None);
        drain_to(&mut session, 0);
        // The current controller's own Instant can target Elenda.
        cast(&mut session, actor, bolt, Some(elenda));
        let pending = session.snapshot_v52();
        cast(&mut session, actor.opponent(), employment, Some(elenda));
        let changed = drain_to(&mut session, 1);
        assert_eq!(
            row(&changed, elenda).source.controller,
            actor.opponent().into()
        );
        assert_stats(&changed, elenda, 4, false);
        let control_snapshot = session.snapshot_v52();
        cast(&mut session, actor.opponent(), witness, Some(elenda));
        let suppressed = drain_to(&mut session, 1);
        let lost = row(&suppressed, elenda);
        assert!(
            !lost.printed_abilities_active
                && !lost.effective_lifelink
                && !lost.hexproof_from_opponents_instants
        );
        assert_eq!(
            (lost.effective_power, lost.effective_toughness),
            (Some(1), Some(1))
        );
        let final_public = drain_to(&mut session, 0);
        assert_eq!(
            session.inner.state.objects.get(elenda).zone,
            Zone::Graveyard
        );
        let first = gameplay(&session);
        session.restore_v52(&control_snapshot).unwrap();
        cast(&mut session, actor.opponent(), witness, Some(elenda));
        assert_eq!(drain_to(&mut session, 0), final_public);
        assert_eq!(gameplay(&session), first);
        session.restore_v52(&pending).unwrap();
        cast(&mut session, actor.opponent(), employment, Some(elenda));
        drain_to(&mut session, 0);
        // After control changes the pending opposing Instant loses its target.
        assert_eq!(session.inner.state.objects.get(elenda).damage, 0);
        assert_eq!(
            session.inner.state.objects.get(elenda).zone,
            Zone::Battlefield
        );
    }
}
#[test]
fn genuine_paid_copy_legend_choice_restore_and_printed_suppression_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let reflection = put(&mut state, actor, "Self-Reflection", Zone::Hand);
        let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
        let wings = put(&mut state, actor, "Broken Wings", Zone::Hand);
        let mut session = own(state);
        cast(&mut session, actor, elenda, None);
        drain_to(&mut session, 0);
        cast(&mut session, actor, reflection, Some(elenda));
        for _ in 0..16 {
            if session
                .decision_v52()
                .unwrap()
                .actions
                .iter()
                .any(|a| matches!(a, ActionSemanticV1::ChooseLegendPermanent { .. }))
            {
                break;
            }
            act(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        }
        let choice = session.snapshot_v52();
        let copy = session
            .decision_v52()
            .unwrap()
            .actions
            .iter()
            .find_map(|a| match a {
                ActionSemanticV1::ChooseLegendPermanent { keep, .. }
                    if keep.arena_id != elenda.0 =>
                {
                    Some(ObjectId(keep.arena_id))
                }
                _ => None,
            })
            .expect("real legendary token choice");
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ChooseLegendPermanent { keep, .. } if keep.arena_id == copy.0),
        );
        let public = drain_to(&mut session, 0);
        assert_stats(&public, copy, 10, true);
        assert!(row(&public, copy).token);
        assert_eq!(
            session.inner.state.objects.get(elenda).zone,
            Zone::Graveyard
        );
        let first = gameplay(&session);
        session.restore_v52(&choice).unwrap();
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ChooseLegendPermanent { keep, .. } if keep.arena_id == copy.0),
        );
        assert_eq!(drain_to(&mut session, 0), public);
        assert_eq!(gameplay(&session), first);
        cast(&mut session, actor, witness, Some(copy));
        let lost = drain_to(&mut session, 0);
        assert!(!row(&lost, copy).printed_abilities_active);
        cast(&mut session, actor, wings, Some(witness));
        assert_stats(&drain_to(&mut session, 0), copy, 10, true);
    }
}

#[test]
fn sealed_full_binding_foreign_stale_forged_and_exact_native_order_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let mut foreign = own(state.clone());
        let foreign_snapshot = foreign.snapshot_v52();
        let foreign_decision = foreign.decision_v52().unwrap();
        let mut session = own(state);
        let original = session.decision_v52().unwrap();
        let snapshot = session.snapshot_v52();
        let before = gameplay(&session);
        assert!(session.restore_v52(&foreign_snapshot).is_err());
        assert!(session.step_v52(&foreign_decision, 0).is_err());
        assert_eq!(gameplay(&session), before);
        let mut forged_snapshot = snapshot.clone();
        forged_snapshot
            .inner
            .0
            .current
            .as_mut()
            .unwrap()
            .candidates
            .reverse();
        assert!(session.restore_v52(&forged_snapshot).is_err());
        assert_eq!(gameplay(&session), before);
        let mut forged_snapshot = snapshot.clone();
        forged_snapshot
            .inner
            .0
            .current
            .as_mut()
            .unwrap()
            .environment_revision += 1;
        assert!(session.restore_v52(&forged_snapshot).is_err());
        assert_eq!(gameplay(&session), before);
        let mut forgeries = Vec::new();
        let mut forged = original.clone();
        forged.expected.environment_revision += 1;
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.generation += 1;
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.observation.elenda.meaning.model_admitted = true;
        forged.observation.projection_sha256 =
            crate::elenda_observation_v52::commitment(&forged.observation).unwrap();
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.binding.observation.common.step_index += 1;
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.binding.candidates.reverse();
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.actions.reverse();
        forgeries.push(forged);
        let mut forged = original.clone();
        forged.binding.families.clear();
        forgeries.push(forged);
        for forged in forgeries {
            assert!(session.validate_decision_v52(&forged).is_err());
            assert!(session.step_v52(&forged, 0).is_err());
            assert_eq!(gameplay(&session), before);
        }
        assert!(session
            .step_v52(&original, original.actions.len() as u32)
            .is_err());
        assert_eq!(gameplay(&session), before);
        // The sealed index is the original native index, not a sorted display menu.
        let selected = original.actions.iter().position(|a| matches!(a, ActionSemanticV1::CastSpell { source, .. } if source.arena_id == elenda.0)).unwrap();
        let mut native = session.inner.clone();
        let expected = native
            .step(
                original.expected.episode_id,
                original.expected.step,
                selected as u32,
            )
            .unwrap();
        assert_eq!(
            session.step_v52(&original, selected as u32).unwrap(),
            expected
        );
        assert_eq!(session.inner.state, native.state);
        assert!(session.step_v52(&original, selected as u32).is_err());
        session.restore_v52(&snapshot).unwrap();
        assert!(session.validate_decision_v52(&original).is_err());
        let current = session.decision_v52().unwrap();
        assert!(current.expected.environment_revision > original.expected.environment_revision);
        assert_eq!(current.actions, original.actions);
        assert_eq!(current.observation, original.observation);
        // Identical public content does not transfer ownership in either direction.
        assert!(foreign.validate_decision_v52(&current).is_err());
        assert!(foreign.restore_v52(&snapshot).is_err());
    }
}
#[test]
fn paid_independent_hidden_worlds_public_validation_and_dto_forgery_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hidden_hand: &str, hidden_library: &str| {
            let mut state = raw(actor, 30);
            let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
            put(&mut state, actor.opponent(), hidden_hand, Zone::Hand);
            put(&mut state, actor.opponent(), hidden_library, Zone::Library);
            (own(state), elenda)
        };
        let (mut a, elenda_a) = build("Lightning Bolt", "Forest");
        let (mut b, elenda_b) = build("Witness Protection", ELENDA);
        assert_ne!(a.inner.state, b.inner.state);
        cast(&mut a, actor, elenda_a, None);
        cast(&mut b, actor, elenda_b, None);
        let public_a = drain_to(&mut a, 0);
        let public_b = drain_to(&mut b, 0);
        assert_eq!(public_a, public_b);
        assert_eq!(
            a.decision_v52().unwrap().actions,
            b.decision_v52().unwrap().actions
        );
        let current = a.inner.current.as_ref().unwrap();
        let validate = |value: &ElendaObservationV52| {
            crate::elenda_observation_v52::validate_current(
                &a.inner.state,
                &a.inner.surface,
                (current.actor, &current.origin_decision),
                a.inner.policy_step_count,
                current.physical_decision_id,
                (current.substep_index, current.substep_count),
                value,
            )
        };
        assert!(validate(&public_a).is_ok());
        let before = gameplay(&a);
        let mut mutations = Vec::new();
        let mut forged = public_a.clone();
        forged.elenda.current[0].controller_life += 1;
        mutations.push(forged);
        let mut forged = public_a.clone();
        forged.elenda.current[0].ordered_bonuses.reverse();
        mutations.push(forged);
        let mut forged = public_a.clone();
        forged.elenda.current[0].hexproof_from_opponents_instants = false;
        mutations.push(forged);
        let mut forged = public_a.clone();
        forged.elenda.current[0].source.zone_change_count += 1;
        mutations.push(forged);
        let mut forged = public_a.clone();
        forged.common.battlefield.reverse();
        mutations.push(forged);
        for mut forged in mutations {
            forged.projection_sha256 = crate::elenda_observation_v52::commitment(&forged).unwrap();
            assert!(validate(&forged).is_err());
            assert_eq!(gameplay(&a), before);
        }
        let text = serde_json::to_string(&public_b).unwrap();
        for private in [
            "prebatch",
            "private_snapshot",
            "source_type_inputs",
            "operation_world",
            "historical_source",
        ] {
            assert!(!text.contains(private));
        }
    }
}
#[test]
fn predecessor_visible_boundary_and_hidden_noninterference_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let old = FastActorSessionV1::from_ravenous_v51_fixture_state(state.clone());
        assert!(matches!(
            old.current_response(),
            FastActorResponseV1::Terminal(_)
        ));
        let mut new = own(state);
        for on_battlefield in [false, true] {
            if on_battlefield {
                cast(&mut new, actor, elenda, None);
                drain_to(&mut new, 0);
            }
            let c = new.inner.current.as_ref().unwrap();
            let before = gameplay(&new);
            assert!(crate::ravenous_observation_v51::observe_v51(
                &new.inner.state,
                &new.inner.surface,
                (actor, &c.origin_decision),
                new.inner.policy_step_count,
                c.physical_decision_id,
                (c.substep_index, c.substep_count)
            )
            .is_err());
            assert!(crate::rl::observe_v1(&new.inner.state, actor, 0).is_err());
            assert!(new
                .inner
                .ravenous_current_decision_input_v51(
                    new.decision_v52().unwrap().expected,
                    actor.into()
                )
                .is_err());
            assert!(new
                .inner
                .encode_current_flat_action_slice_v1(
                    new.decision_v52().unwrap().expected,
                    &mut FlatActionDecisionSliceBuffersV1 {
                        actions: &mut [],
                        refs: &mut [],
                        objects: &mut []
                    }
                )
                .is_err());
            assert_eq!(gameplay(&new), before);
        }
        let make_hidden = |printing: &str| {
            let mut state = raw(actor, 20);
            put(&mut state, actor.opponent(), printing, Zone::Hand);
            state
        };
        let hidden = make_hidden(ELENDA);
        assert!(crate::elenda_public_v52::require_legacy_visible(&hidden, Some(actor)).is_ok());
        assert!(
            crate::elenda_public_v52::require_legacy_visible(&hidden, Some(actor.opponent()))
                .is_err()
        );
        let a = FastActorSessionV1::from_ravenous_v51_fixture_state(hidden);
        let b = FastActorSessionV1::from_ravenous_v51_fixture_state(make_hidden("Forest"));
        let FastActorResponseV1::Decision(ca) = a.current_response() else {
            panic!("hidden Elenda changed refusal")
        };
        let FastActorResponseV1::Decision(cb) = b.current_response() else {
            panic!("hidden Forest changed refusal")
        };
        assert_eq!(
            a.ravenous_current_decision_input_v51(ca, actor.into())
                .unwrap()
                .0,
            b.ravenous_current_decision_input_v51(cb, actor.into())
                .unwrap()
                .0
        );
        // Catalog capability remains an independent public reset limitation.
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut deck = vec![forest; 40];
        deck[0] = crate::card_def::card_id_by_name(ELENDA).unwrap();
        assert!(ElendaSessionV52::reset(
            23,
            91,
            (10000, 10000),
            ["a".into(), "b".into()],
            [deck, vec![forest; 40]],
            actor
        )
        .is_err());
    }
}

#[test]
fn genuine_paid_elenda_natural_combat_lifelink_thresholds_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 20);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let mut session = own(state);
        cast(&mut session, actor, elenda, None);
        drain_to(&mut session, 0);
        let saved = session.snapshot_v52();
        let play = |session: &mut ElendaSessionV52| {
            let mut seen = Vec::new();
            for _ in 0..1024 {
                let decision = session.decision_v52().unwrap();
                let life = session.inner.state.players[actor.index()].life;
                if seen.last().map(|&(previous, _)| previous) != Some(life) && life > 20 {
                    let pt = row(&decision.observation, elenda).effective_power.unwrap();
                    seen.push((life, pt));
                    assert!(row(&decision.observation, elenda).effective_lifelink);
                }
                if life >= 34 {
                    return (decision.observation, seen);
                }
                let chosen = decision
                    .actions
                    .iter()
                    .position(|action| match action {
                        ActionSemanticV1::ChooseAttackerInclusion {
                            attacker, include, ..
                        } => *include == (attacker.arena_id == elenda.0),
                        ActionSemanticV1::DeclareAttackers { attackers, .. } => {
                            attackers.len() == 1 && attackers[0].arena_id == elenda.0
                        }
                        ActionSemanticV1::DeclareBlockersForAttacker { blockers, .. } => {
                            blockers.is_empty()
                        }
                        ActionSemanticV1::ChooseBlockerInclusion { include, .. } => !include,
                        ActionSemanticV1::ChooseAttackTarget {
                            target: TargetRefV1::Player { player },
                            ..
                        } => *player == actor.opponent().into(),
                        ActionSemanticV1::Pass { .. } => true,
                        _ => false,
                    })
                    .or_else(|| (decision.actions.len() == 1).then_some(0))
                    .expect("actual combat choice");
                session.step_v52(&decision, chosen as u32).unwrap();
            }
            panic!("natural owning combat did not reach lifelink thresholds");
        };
        let (public, thresholds) = play(&mut session);
        assert_eq!(thresholds, [(24, 5), (29, 5), (34, 10)]);
        assert_stats(&public, elenda, 10, true);
        assert_eq!(
            session.inner.state.players[actor.opponent().index()].life,
            6
        );
        let first = gameplay(&session);
        session.restore_v52(&saved).unwrap();
        assert_eq!(play(&mut session), (public, thresholds));
        assert_eq!(gameplay(&session), first);
    }
}

#[test]
fn genuine_opponent_instant_target_menu_refuses_elenda_and_pays_for_actual_legal_choice_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        let bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let mut session = own(state);
        cast(&mut session, actor, elenda, None);
        drain_to(&mut session, 0);
        priority(&mut session, actor.opponent());
        let red_before = session.inner.state.players[actor.opponent().index()].mana_pool
            [crate::mana::ManaColor::R.pool_index()];
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::CastSpell { source, .. } if source.arena_id == bolt.0),
        );
        let menu = session.decision_v52().unwrap();
        assert!(!menu.actions.iter().any(|a| matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == elenda.0)));
        assert!(row(&menu.observation, elenda).hexproof_from_opponents_instants);
        let saved = session.snapshot_v52();
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } if *player == actor.opponent().into()),
        );
        assert_eq!(
            session.inner.state.players[actor.opponent().index()].mana_pool
                [crate::mana::ManaColor::R.pool_index()],
            red_before - 1
        );
        let public = drain_to(&mut session, 0);
        assert_eq!(
            session.inner.state.players[actor.opponent().index()].life,
            17
        );
        assert_stats(&public, elenda, 10, true);
        session.restore_v52(&saved).unwrap();
        act(
            &mut session,
            |a| matches!(a, ActionSemanticV1::ChooseTarget { target: TargetRefV1::Player { player }, .. } if *player == actor.opponent().into()),
        );
        assert_eq!(drain_to(&mut session, 0), public);
    }
}
