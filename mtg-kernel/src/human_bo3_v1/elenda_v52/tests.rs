//! Genuine Human action-path source. Initial life/floating mana and High Fae
//! support are fixture inputs; original private journals are never rewritten.
use super::*;
use crate::{
    ids::PlayerId,
    policy_observation_v6::tests::put,
    rl::TargetRefV1,
    state::{GameState, Zone},
};
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
fn own(state: GameState, actor: PlayerId) -> HumanElendaSessionV52 {
    HumanElendaSessionV52::from_session(
        ElendaSessionV52::from_session(FastActorSessionV1::from_elenda_v52_fixture_state(state))
            .unwrap(),
        actor.into(),
    )
}
fn expected(session: &HumanElendaSessionV52) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(value) = session.current_response() else {
        panic!("genuine Human V52 menu absent")
    };
    value
}
fn menu(
    session: &HumanElendaSessionV52,
    projector: &mut HumanElendaProjectorV52,
) -> HumanDecisionElendaV52 {
    projector
        .project_current(session, expected(session))
        .unwrap()
}
fn submit(
    session: &mut HumanElendaSessionV52,
    projector: &mut HumanElendaProjectorV52,
    choose: impl Fn(&str) -> bool,
) -> HumanActionRequestV1 {
    let public = menu(session, projector);
    let row = public
        .actions
        .iter()
        .find(|row| choose(&row.label))
        .unwrap_or_else(|| panic!("actual Human choice absent:{:?}", public.actions));
    let request = HumanActionRequestV1 {
        prompt_seq: public.prompt_seq,
        action_index: row.action_index,
    };
    projector.submit(session, request.clone()).unwrap();
    request
}
fn opponent(session: &mut HumanElendaSessionV52, choose: impl Fn(&ActionSemanticV1) -> bool) {
    session
        .step_opponent_with(expected(session), |_, actions| {
            actions
                .iter()
                .position(&choose)
                .map(|i| i as u32)
                .ok_or_else(|| RlContractError("actual opponent choice absent".into()))
        })
        .unwrap();
}
fn priority(session: &mut HumanElendaSessionV52) {
    for _ in 0..8 {
        if expected(session).acting_player == session.human_seat {
            return;
        }
        opponent(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
    }
    panic!("Human priority absent");
}
fn cast(
    session: &mut HumanElendaSessionV52,
    projector: &mut HumanElendaProjectorV52,
    name: &str,
    target: Option<&str>,
) {
    priority(session);
    let actor = match session.human_seat {
        PlayerSeatV1::P0 => PlayerId::P0,
        PlayerSeatV1::P1 => PlayerId::P1,
    };
    let before: u32 = session.inner.fixture_state_v52().players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    submit(session, projector, |label| {
        label.starts_with(&format!("Begin casting {name} ["))
    });
    if let Some(target) = target {
        submit(session, projector, |label| {
            label.starts_with(&format!("Target {target} ["))
                || (label.starts_with(&format!("{name} ["))
                    && label.contains(&format!(": target {target} [")))
        });
    }
    let after: u32 = session.inner.fixture_state_v52().players[actor.index()]
        .mana_pool
        .iter()
        .map(|&v| u32::from(v))
        .sum();
    let id = crate::card_def::card_id_by_name(name).unwrap();
    assert_eq!(
        before - after,
        u32::from(crate::card_def::CARD_DEFS[usize::from(id)].mana_value)
    );
}
fn drain_to(
    session: &mut HumanElendaSessionV52,
    projector: &mut HumanElendaProjectorV52,
    remaining: usize,
) -> HumanDecisionElendaV52 {
    for _ in 0..64 {
        if expected(session).acting_player != session.human_seat {
            opponent(session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
        } else if session.inner.fixture_state_v52().stack.len() == remaining
            && session
                .inner
                .fixture_state_v52()
                .engine
                .pending_triggers
                .is_empty()
        {
            return menu(session, projector);
        } else {
            submit(session, projector, |label| label == "Pass priority");
        }
    }
    panic!("Human stack did not finish");
}
fn gameplay(session: &HumanElendaSessionV52) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(session.inner.fixture_state_v52()).unwrap(),
        session.inner.fixture_counts_v52(),
    )
}
fn traits(public: &HumanDecisionElendaV52) -> &HumanElendaTraitsV52 {
    &public.state.elenda.current[0].traits
}
fn stats(public: &HumanDecisionElendaV52, power: i32, menace: bool) {
    assert_eq!(
        (
            traits(public).effective_power,
            traits(public).effective_toughness
        ),
        (Some(power), Some(power))
    );
    assert_eq!(traits(public).effective_menace, menace);
    assert!(traits(public).effective_lifelink && traits(public).hexproof_from_opponents_instants);
    assert_eq!(public.state.elenda.meaning.ordered_rules.len(), 4);
    assert_eq!(
        traits(public)
            .ordered_bonuses
            .iter()
            .map(|bonus| bonus.minimum_above_start)
            .collect::<Vec<_>>(),
        [1, 10]
    );
}
fn safe(public: &HumanDecisionElendaV52) {
    let text = serde_json::to_string(public).unwrap();
    for private in [
        "arena_id",
        "card_db_id",
        "zone_change_count",
        "root_sha256",
        "producer_sha256",
        "card_db_hash",
        "engine_indexes",
        "prebatch",
        "private_snapshot",
        "original_world",
        "event_history",
    ] {
        assert!(!text.contains(private), "private Human field:{private}");
    }
}
#[test]
fn genuine_human_paid_thresholds_restore_retry_foreign_stale_and_issued_label_authority_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (life, power, menace) in [(20, 4, false), (21, 5, true), (29, 5, true), (30, 10, true)]
        {
            let mut state = raw(actor, life);
            put(&mut state, actor, ELENDA, Zone::Hand);
            let mut foreign = own(state.clone(), actor);
            let foreign_snapshot = foreign.snapshot_v52();
            let mut session = own(state, actor);
            let mut projector = HumanElendaProjectorV52::new(actor.into());
            let current = menu(&session, &mut projector);
            assert_eq!(menu(&session, &mut projector), current);
            safe(&current);
            let row = current
                .actions
                .iter()
                .find(|r| r.label.starts_with("Begin casting Elenda, Saint of Dusk ["))
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: current.prompt_seq,
                action_index: row.action_index,
            };
            let before = gameplay(&session);
            assert!(projector.submit(&mut foreign, request.clone()).is_err());
            assert!(session.restore_v52(&foreign_snapshot).is_err());
            assert_eq!(gameplay(&session), before);
            // A client-edited DTO never replaces the privately issued menu.
            let mut edited = current.clone();
            edited.actions[request.action_index as usize].label = "forged free cast".into();
            assert_ne!(edited, current);
            // Even private corruption of labels or menu-to-native indexes refuses.
            projector.pending.as_mut().unwrap().visible.actions[0]
                .label
                .push_str(" forged");
            assert!(projector.submit(&mut session, request.clone()).is_err());
            assert_eq!(gameplay(&session), before);
            projector.pending.as_mut().unwrap().visible = current.clone();
            projector.pending.as_mut().unwrap().indexes.reverse();
            assert!(projector.submit(&mut session, request.clone()).is_err());
            assert_eq!(gameplay(&session), before);
            projector.pending.as_mut().unwrap().indexes.reverse();
            projector
                .pending
                .as_mut()
                .unwrap()
                .visible
                .state
                .elenda
                .meaning
                .supported_starting_life = 40;
            assert!(projector.submit(&mut session, request.clone()).is_err());
            assert_eq!(gameplay(&session), before);
            projector.pending.as_mut().unwrap().visible = current.clone();
            let mana_before: u32 = session.inner.fixture_state_v52().players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum();
            projector.submit(&mut session, request.clone()).unwrap();
            let mana_after: u32 = session.inner.fixture_state_v52().players[actor.index()]
                .mana_pool
                .iter()
                .map(|&v| u32::from(v))
                .sum();
            assert_eq!(mana_before - mana_after, 4);
            let paid = session.snapshot_v52();
            let after = gameplay(&session);
            assert_eq!(
                projector.submit(&mut session, request.clone()).unwrap(),
                HumanActionReceiptV1 {
                    prompt_seq: request.prompt_seq,
                    action_index: request.action_index
                }
            );
            assert_eq!(gameplay(&session), after);
            let mut conflict = request.clone();
            conflict.action_index = (request.action_index + 1) % current.actions.len() as u32;
            assert_eq!(
                projector.submit(&mut session, conflict).unwrap_err(),
                Error::ConflictingRetry
            );
            let public = drain_to(&mut session, &mut projector, 0);
            stats(&public, power, menace);
            safe(&public);
            let first = gameplay(&session);
            session.restore_v52(&paid).unwrap();
            assert!(projector.submit(&mut session, request.clone()).is_err());
            let replay = drain_to(&mut session, &mut projector, 0);
            assert_eq!(replay.state, public.state);
            assert_eq!(replay.actions, public.actions);
            assert_eq!(gameplay(&session), first);
            let mut wrong_seat = HumanElendaProjectorV52::new(actor.opponent().into());
            assert_eq!(
                wrong_seat
                    .project_current(&session, expected(&session))
                    .unwrap_err(),
                Error::NotHumanTurn
            );
        }
    }
}
#[test]
fn genuine_human_paid_copy_legend_choice_ability_suppression_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        put(&mut state, actor, "Self-Reflection", Zone::Hand);
        put(&mut state, actor, "Witness Protection", Zone::Hand);
        put(&mut state, actor, "Broken Wings", Zone::Hand);
        let mut session = own(state, actor);
        let mut projector = HumanElendaProjectorV52::new(actor.into());
        cast(&mut session, &mut projector, ELENDA, None);
        drain_to(&mut session, &mut projector, 0);
        cast(
            &mut session,
            &mut projector,
            "Self-Reflection",
            Some(ELENDA),
        );
        for _ in 0..16 {
            if expected(&session).acting_player != session.human_seat {
                opponent(&mut session, |a| matches!(a, ActionSemanticV1::Pass { .. }));
            } else if menu(&session, &mut projector)
                .actions
                .iter()
                .any(|a| a.label.starts_with("Keep "))
            {
                break;
            } else {
                submit(&mut session, &mut projector, |label| {
                    label == "Pass priority"
                });
            }
        }
        let saved = session.snapshot_v52();
        let choice = menu(&session, &mut projector);
        let copy = choice
            .state
            .elenda
            .current
            .iter()
            .find(|r| r.traits.token)
            .unwrap()
            .source
            .handle
            .clone();
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Keep ") && label.contains(&format!("[{copy}]"))
        });
        let public = drain_to(&mut session, &mut projector, 0);
        stats(&public, 10, true);
        assert!(traits(&public).token);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).zone,
            Zone::Graveyard
        );
        let first = gameplay(&session);
        session.restore_v52(&saved).unwrap();
        let choice = menu(&session, &mut projector);
        let copy = choice
            .state
            .elenda
            .current
            .iter()
            .find(|r| r.traits.token)
            .unwrap()
            .source
            .handle
            .clone();
        submit(&mut session, &mut projector, |label| {
            label.starts_with("Keep ") && label.contains(&format!("[{copy}]"))
        });
        assert_eq!(
            drain_to(&mut session, &mut projector, 0).state,
            public.state
        );
        assert_eq!(gameplay(&session), first);
        cast(
            &mut session,
            &mut projector,
            "Witness Protection",
            Some(ELENDA),
        );
        let lost = drain_to(&mut session, &mut projector, 0);
        assert!(
            !traits(&lost).printed_abilities_active
                && !traits(&lost).effective_lifelink
                && !traits(&lost).hexproof_from_opponents_instants
        );
        assert_eq!(traits(&lost).effective_power, Some(1));
        assert!(traits(&lost)
            .ordered_bonuses
            .iter()
            .all(|bonus| bonus.condition_satisfied && !bonus.applied));
        cast(
            &mut session,
            &mut projector,
            "Broken Wings",
            Some("Witness Protection"),
        );
        stats(&drain_to(&mut session, &mut projector, 0), 10, true);
    }
}
#[test]
fn genuine_human_instant_target_refusal_and_pending_control_suppression_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        let elenda = put(&mut state, actor, ELENDA, Zone::Hand);
        put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        let enemy_bolt = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
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
        let mut session = own(state, actor);
        let mut projector = HumanElendaProjectorV52::new(actor.into());
        cast(&mut session, &mut projector, ELENDA, None);
        drain_to(&mut session, &mut projector, 0);
        // Actual opponent cast/target path proves the Human's protection is selective.
        submit(&mut session, &mut projector, |label| {
            label == "Pass priority"
        });
        opponent(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==enemy_bolt.0),
        );
        session.step_opponent_with(expected(&session),|_,actions|{
            assert!(!actions.iter().any(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elenda.0)));
            actions.iter().position(|a|matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Player{player},..}if *player==actor.opponent().into())).map(|i|i as u32).ok_or_else(||RlContractError("actual legal Bolt target absent".into()))
        }).unwrap();
        drain_to(&mut session, &mut projector, 0);
        cast(&mut session, &mut projector, "Lightning Bolt", Some(ELENDA));
        let paid = session.snapshot_v52();
        let change = |session: &mut HumanElendaSessionV52,
                      projector: &mut HumanElendaProjectorV52| {
            if expected(session).acting_player == session.human_seat {
                submit(session, projector, |label| label == "Pass priority");
            }
            opponent(
                session,
                |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==employment.0),
            );
            opponent(
                session,
                |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elenda.0),
            );
            drain_to(session, projector, 1)
        };
        let changed = change(&mut session, &mut projector);
        assert_eq!(
            changed.state.elenda.current[0].source.controller,
            actor.opponent().into()
        );
        stats(&changed, 4, false);
        let control = session.snapshot_v52();
        submit(&mut session, &mut projector, |label| {
            label == "Pass priority"
        });
        opponent(
            &mut session,
            |a| matches!(a,ActionSemanticV1::CastSpell{source,..}if source.arena_id==witness.0),
        );
        opponent(
            &mut session,
            |a| matches!(a,ActionSemanticV1::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==elenda.0),
        );
        let lost = drain_to(&mut session, &mut projector, 1);
        assert!(!traits(&lost).printed_abilities_active);
        assert_eq!(traits(&lost).effective_power, Some(1));
        drain_to(&mut session, &mut projector, 0);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).zone,
            Zone::Graveyard
        );
        session.restore_v52(&control).unwrap();
        drain_to(&mut session, &mut projector, 0);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).zone,
            Zone::Battlefield
        );
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).damage,
            0
        );
        session.restore_v52(&paid).unwrap();
        change(&mut session, &mut projector);
        drain_to(&mut session, &mut projector, 0);
        assert_eq!(
            session.inner.fixture_state_v52().objects.get(elenda).damage,
            0
        );
    }
}
#[test]
fn genuine_human_paid_elenda_fake_coexistence_exact_widths_and_independent_hidden_worlds_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |hand: &str, library: &str| {
            let mut state = raw(actor, 30);
            // These are independent initial worlds, before retained custody inputs.
            put(&mut state, actor.opponent(), hand, Zone::Hand);
            put(&mut state, actor.opponent(), library, Zone::Library);
            put(&mut state, actor, ELENDA, Zone::Hand);
            put(&mut state, actor, "Fake Your Own Death", Zone::Hand);
            let amulet = put(&mut state, actor, "Ravenous Amulet", Zone::Battlefield);
            // Honest primitive width input, not a claimed paid counter placement.
            state.objects.get_mut(amulet).counters.soul = 65537;
            state.objects.get_mut(amulet).counters.plus1_plus1 = 65538;
            state.objects.get_mut(amulet).damage = 70000;
            let mut session = own(state, actor);
            let mut projector = HumanElendaProjectorV52::new(actor.into());
            cast(&mut session, &mut projector, ELENDA, None);
            drain_to(&mut session, &mut projector, 0);
            cast(
                &mut session,
                &mut projector,
                "Fake Your Own Death",
                Some(ELENDA),
            );
            let public = drain_to(&mut session, &mut projector, 0);
            (session, projector, public)
        };
        let (a, mut pa, public_a) = build("Lightning Bolt", "Forest");
        let (b, mut pb, public_b) = build("Witness Protection", ELENDA);
        assert_ne!(gameplay(&a), gameplay(&b));
        assert_eq!(public_a, public_b);
        safe(&public_a);
        assert_eq!(menu(&a, &mut pa), menu(&b, &mut pb));
        assert!(!public_a.state.predecessor.extensions.fake.grants.is_empty());
        assert_eq!(
            public_a.state.predecessor.extensions.ravenous.soul_counts[0].count,
            65537
        );
        let exact = public_a
            .state
            .predecessor
            .public
            .battlefield
            .iter()
            .flatten()
            .find(|row| row.card.stable.name == "Ravenous Amulet")
            .unwrap();
        assert_eq!(
            (exact.soul, exact.plus_one_counters, exact.marked_damage),
            (65537, 65538, 70000)
        );
        // Old fixed-seat Human V51 cannot publish visible Elenda mechanics.
        let old = super::super::ravenous_v51::HumanRavenousSessionV51::from_session(
            FastActorSessionV1::from_ravenous_v51_fixture_state(
                a.inner.fixture_state_v52().clone(),
            ),
            actor.into(),
        )
        .unwrap();
        let mut old_projector =
            super::super::ravenous_v51::HumanRavenousProjectorV51::new(actor.into());
        assert!(old_projector.project_current(&old, expected(&a)).is_err());
    }
}

#[test]
fn human_paid_neutral_face_copy_hides_independent_underlying_printings_and_handles_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |printing: &str| {
            let mut state = raw(actor, 21);
            // Pool has no admitted printed face-down producer: this initial
            // neutral face is a primitive operand, never a fabricated printing.
            // Self-Reflection targets a creature the caster controls. No
            // viewer has looked at this initial neutral-face primitive.
            let hidden = put(&mut state, actor, printing, Zone::Battlefield);
            state.objects.get_mut(hidden).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            put(&mut state, actor, ELENDA, Zone::Hand);
            put(&mut state, actor, "Self-Reflection", Zone::Hand);
            let mut session = own(state, actor);
            let mut projector = HumanElendaProjectorV52::new(actor.into());
            cast(&mut session, &mut projector, ELENDA, None);
            drain_to(&mut session, &mut projector, 0);
            cast(
                &mut session,
                &mut projector,
                "Self-Reflection",
                Some("Face-down creature"),
            );
            let paid_state = session.inner.fixture_state_v52();
            assert_eq!(paid_state.objects.get(hidden).controller, actor);
            let spell = paid_state
                .stack
                .iter()
                .find(|item| {
                    crate::card_def::CARD_DEFS
                        [usize::from(paid_state.objects.get(item.source).card_def)]
                    .name
                        == "Self-Reflection"
                })
                .unwrap();
            assert_eq!(spell.controller, actor);
            assert_eq!(spell.targets, [crate::state::Target::Object(hidden)]);
            assert_eq!(spell.v4.target_contracts.len(), 1);
            let paid = session.snapshot_v52();
            let public = drain_to(&mut session, &mut projector, 0);
            assert_eq!(public.state.elenda.current.len(), 1);
            stats(&public, 5, true);
            safe(&public);
            assert!(public.state.predecessor.known_face_down_cards.is_empty());
            assert!(public
                .state
                .predecessor
                .public
                .battlefield
                .iter()
                .flatten()
                .any(|row| row.card.is_token && row.card.stable.name.is_empty()));
            let completed = gameplay(&session);
            session.restore_v52(&paid).unwrap();
            let replayed = drain_to(&mut session, &mut projector, 0);
            assert_eq!(replayed.state, public.state);
            assert_eq!(replayed.actions, public.actions);
            assert_eq!(gameplay(&session), completed);
            public
        };
        assert_eq!(build(ELENDA), build("Llanowar Elves"));
    }
}
#[test]
fn human_invalid_index_stale_header_opponent_callback_and_prompt_exhaustion_are_readonly_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = raw(actor, 30);
        put(&mut state, actor, ELENDA, Zone::Hand);
        let mut session = own(state, actor);
        let mut projector = HumanElendaProjectorV52::new(actor.into());
        let public = menu(&session, &mut projector);
        let before = gameplay(&session);
        let invalid = HumanActionRequestV1 {
            prompt_seq: public.prompt_seq,
            action_index: public.actions.len() as u32,
        };
        assert_eq!(
            projector.submit(&mut session, invalid).unwrap_err(),
            Error::InvalidAction
        );
        let mut header = expected(&session);
        header.environment_revision += 1;
        assert_eq!(
            projector.project_current(&session, header).unwrap_err(),
            Error::StaleDecision
        );
        let expected = expected(&session);
        assert!(session
            .step_opponent_with(expected, |_, _| panic!(
                "Human private input reached opponent callback"
            ))
            .is_err());
        projector.next_prompt_seq = u64::MAX;
        projector.pending = None;
        assert_eq!(
            projector.project_current(&session, expected).unwrap_err(),
            Error::PromptSequenceExhausted
        );
        assert_eq!(gameplay(&session), before);
    }
}
