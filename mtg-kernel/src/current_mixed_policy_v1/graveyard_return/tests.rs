//! Paid native V61 stages through the distinct numeric owner. The native clone
//! is an assertion oracle; only authenticated public DTOs enter numeric packets.
use super::*;
use crate::{
    graveyard_return_public_v61::{tests as native, ProgramV61},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::{GameState, Zone},
};
use serde_json::json;

fn own(state: GameState) -> GraveyardReturnSession {
    GraveyardReturnSession::from_session(
        FastActorSessionV1::from_graveyard_return_v61_fixture_state(state),
    )
    .unwrap()
}
fn current(owner: &GraveyardReturnSession) -> GraveyardReturnDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("native V61 fixture became terminal")
    };
    owner.encode(expected).unwrap()
}
fn gameplay(owner: &GraveyardReturnSession) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(owner.fixture_state()).unwrap(),
        owner.fixture_counts(),
    )
}
fn act(owner: &mut GraveyardReturnSession, choose: impl Fn(&A) -> bool) {
    let decision = current(owner);
    let ordinal = decision
        .backend
        .actions()
        .iter()
        .position(choose)
        .unwrap_or_else(|| panic!("native V61 action absent: {:?}", decision.backend.actions()));
    let ordinal = u32::try_from(ordinal).unwrap();
    let original = decision
        .backend
        .fixture_original_index_v61(ordinal)
        .unwrap();
    let expected = decision.backend.expected();
    let mut reference = owner.inner.fixture_native_v61();
    let scores = NativeModel::deterministic_fixture()
        .unwrap()
        .forward(decision.packet())
        .unwrap();
    assert_eq!(scores.len(), decision.backend.actions().len());
    assert!(scores.iter().all(|score| score.is_finite()));
    assert_eq!(decision.packet().source(), Source::GraveyardReturnV61);
    let actual = owner.apply(&decision, ordinal).unwrap();
    assert_eq!(
        actual,
        reference
            .step(expected.episode_id, expected.step, original)
            .unwrap()
    );
    assert_eq!(
        owner.fixture_state(),
        reference.graveyard_return_fixture_state()
    );
    assert_eq!(
        owner.fixture_counts(),
        (
            reference.policy_step_count(),
            reference.physical_decision_count()
        )
    );
}
fn quiet(owner: &mut GraveyardReturnSession) {
    act(owner, |action| {
        matches!(
            action,
            A::Pass { .. }
                | A::OrderTriggers { .. }
                | A::ChooseTriggerOrderNext { .. }
                | A::ChooseAttackerInclusion { include: false, .. }
                | A::ChooseBlockerInclusion { include: false, .. }
                | A::FinishEffectSelection { .. }
        )
    });
}
fn drain(owner: &mut GraveyardReturnSession) {
    for _ in 0..100 {
        let state = owner.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_activation.is_none()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(owner);
    }
    panic!("real V61 native stack did not settle");
}
fn taps(owner: &GraveyardReturnSession, actor: PlayerId) -> usize {
    let state = owner.fixture_state();
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&object| {
            let live = state.objects.get(object);
            live.tapped && crate::card_def::CARD_DEFS[usize::from(live.card_def)].is_land
        })
        .count()
}
fn cast(owner: &mut GraveyardReturnSession, actor: PlayerId, source: ObjectId, mana: usize) {
    let before = taps(owner, actor);
    act(
        owner,
        |a| matches!(a, A::CastSpell { source: row, .. } if row.arena_id == source.0),
    );
    let state = owner.fixture_state();
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.objects.get(source).zone, Zone::Stack);
    assert_eq!(state.stack.last().unwrap().source, source);
    assert_eq!(taps(owner, actor) - before, mana);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}

#[test]
fn paid_both_printings_cast_entry_complete_rules_and_finite_numeric_native_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        let alesha = put(&mut state, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        let mut owner = own(state);
        let decision = current(&owner);
        let facts = &decision.backend.observation().graveyard_return;
        assert_eq!(facts.meanings.len(), 2);
        for meaning in &facts.meanings {
            crate::rules_meaning_graveyard_return_v61::validate(meaning).unwrap();
            assert!(meaning.ordered_rules.len() >= 3);
        }
        cast(&mut owner, actor, alesha, 3);
        drain(&mut owner);
        assert_eq!(
            owner.fixture_state().objects.get(alesha).zone,
            Zone::Battlefield
        );
        cast(&mut owner, actor, familiar, 3);
        assert_eq!(
            owner.fixture_state().objects.get(familiar).counters.revival,
            0
        );
        drain(&mut owner);
        assert_eq!(
            owner.fixture_state().objects.get(familiar).counters.revival,
            8
        );
        let decision = current(&owner);
        assert_eq!(
            decision
                .backend
                .observation()
                .graveyard_return
                .battlefield_counters[actor.index()]
            .iter()
            .find(|row| row.card.object == familiar.0)
            .unwrap()
            .counters
            .revival,
            8
        );
    }
}

#[test]
fn paid_alesha_original_raid_target_costless_accept_decline_and_numeric_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (state, _alesha, target) = native::raid_ready(actor);
            let mut owner = own(state);
            act(
                &mut owner,
                |a| matches!(a, A::ChooseTarget { target: TargetRefV1::Object { object }, .. } if object.arena_id == target.0),
            );
            for _ in 0..40 {
                if current(&owner)
                    .backend
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                quiet(&mut owner);
            }
            let decision = current(&owner);
            assert!(decision
                .backend
                .observation()
                .graveyard_return
                .triggers
                .iter()
                .any(|row| matches!(
                    row.program,
                    ProgramV61::AleshaRaid {
                        may_return_at_resolution: true,
                        ..
                    }
                )));
            let saved = owner.snapshot();
            let packet = decision.packet().clone();
            let before_taps = taps(&owner, actor);
            act(
                &mut owner,
                |a| matches!(a, A::ChooseEffectBoolean { value, .. } if *value == accept),
            );
            drain(&mut owner);
            assert_eq!(
                owner.fixture_state().objects.get(target).zone,
                if accept {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(taps(&owner, actor), before_taps);
            let result = gameplay(&owner);
            owner.restore(&saved).unwrap();
            assert_eq!(current(&owner).packet(), &packet);
            let unchanged = gameplay(&owner);
            assert_eq!(owner.apply(&decision, 0), Err(Error::StaleBinding));
            assert_eq!(gameplay(&owner), unchanged);
            act(
                &mut owner,
                |a| matches!(a, A::ChooseEffectBoolean { value, .. } if *value == accept),
            );
            drain(&mut owner);
            assert_eq!(gameplay(&owner), result);
        }
    }
}

#[test]
fn paid_familiar_death_original_schedule_and_next_endstep_numeric_replay_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        let familiar = native::paid(&mut state, actor, "Nine-Lives Familiar", None);
        let breath = put(&mut state, actor, "Breath Weapon", Zone::Hand);
        let mut owner = own(state);
        cast(&mut owner, actor, breath, 3);
        drain(&mut owner);
        assert_eq!(
            owner.fixture_state().objects.get(familiar).zone,
            Zone::Graveyard
        );
        let decision = current(&owner);
        let schedules = &decision
            .backend
            .observation()
            .graveyard_return
            .nine_lives_schedules;
        assert_eq!(schedules.len(), 1);
        assert_eq!(schedules[0].departure_revival, 8);
        assert_eq!(schedules[0].return_base_before_replacements, 7);
        let saved = owner.snapshot();
        let mut result = None;
        for replay in [false, true] {
            if replay {
                owner.restore(&saved).unwrap();
            }
            for _ in 0..300 {
                let state = owner.fixture_state();
                if state.objects.get(familiar).zone == Zone::Battlefield {
                    assert_eq!(state.objects.get(familiar).counters.revival, 7);
                    break;
                }
                quiet(&mut owner);
            }
            assert_eq!(
                owner.fixture_state().objects.get(familiar).zone,
                Zone::Battlefield
            );
            if let Some(result) = &result {
                assert_eq!(&gameplay(&owner), result);
            } else {
                result = Some(gameplay(&owner));
            }
        }
    }
}

#[test]
fn paid_owner_full_packet_foreign_stale_reordered_erased_and_range_refuse_readonly_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor);
        put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        put(&mut state, actor, "Forest", Zone::Hand);
        let mut owner = own(state.clone());
        let foreign = own(state);
        let decision = current(&owner);
        assert!(decision.packet().actions.len() >= 2);
        let before = gameplay(&owner);
        assert_eq!(owner.apply(&current(&foreign), 0), Err(Error::StaleBinding));
        assert_eq!(gameplay(&owner), before);
        assert_eq!(owner.apply(&decision, u32::MAX), Err(Error::ActionRange));
        assert_eq!(gameplay(&owner), before);
        assert_eq!(
            owner.restore(&foreign.snapshot()),
            Err(Error::ForeignSnapshot)
        );
        assert_eq!(gameplay(&owner), before);
        for corruption in 0..4 {
            let mut changed = decision.clone();
            let packet = changed.fixture_packet_mut();
            match corruption {
                0 => packet.state = super::super::NumericTree::encode(&json!({})).unwrap(),
                1 => packet.actions.clear(),
                2 => packet.actions.swap(0, 1),
                3 => packet.source = Source::NextFiveV59,
                _ => unreachable!(),
            }
            assert!(owner.apply(&changed, 0).is_err());
            assert_eq!(gameplay(&owner), before);
        }
        let saved = owner.snapshot();
        owner.restore(&saved).unwrap();
        let before = gameplay(&owner);
        assert_eq!(owner.apply(&decision, 0), Err(Error::StaleBinding));
        assert_eq!(gameplay(&owner), before);
    }
}

#[test]
fn native_counter_primitive_retains_exact_i32_limbs_and_hidden_printing_privacy_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut base = native::ready(actor);
        let familiar = native::paid(&mut base, actor, "Nine-Lives Familiar", None);
        // Explicit numeric native setup, not a claim of a paid charge placement.
        let mut packets = Vec::new();
        for charge in [16_777_216, 16_777_217] {
            let mut state = base.clone();
            state.objects.get_mut(familiar).counters.charge = charge;
            let owner = own(state);
            let decision = current(&owner);
            assert_eq!(
                decision
                    .backend
                    .observation()
                    .graveyard_return
                    .battlefield_counters[actor.index()]
                .iter()
                .find(|row| row.card.object == familiar.0)
                .unwrap()
                .counters
                .charge,
                charge
            );
            assert!(NativeModel::deterministic_fixture()
                .unwrap()
                .forward(decision.packet())
                .unwrap()
                .iter()
                .all(|score| score.is_finite()));
            packets.push(decision.packet().clone());
        }
        assert_ne!(packets[0], packets[1]);
        let mut hidden = Vec::new();
        for name in ["Alesha, Who Laughs at Fate", "Nine-Lives Familiar"] {
            let mut state = base.clone();
            put(&mut state, actor.opponent(), name, Zone::Hand);
            let owner = own(state);
            let decision = current(&owner);
            assert_eq!(decision.backend.expected().acting_player, actor.into());
            hidden.push(decision.packet().clone());
        }
        assert_eq!(hidden[0], hidden[1]);
    }
}
