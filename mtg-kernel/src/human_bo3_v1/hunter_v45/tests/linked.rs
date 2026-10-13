use super::*;
use crate::effect::{EffectObjectBinding, ExecCtx};
use crate::state::{AbilitySourceContractV4, ObjectLinkV4, Target};

#[test]
#[ignore = "unregistered Hunter; parent owns actual registration/native"]
fn genuine_hunter_sacrifice_and_foreign_aura_owner_prompt_use_distinct_sources_and_resume_after_owning_restore(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, banishing, aura, host) =
            crate::banishing_light_v1::tests::linked_aura(actor);
        let attachment = put(&mut state, actor, "Viridian Longbow", Zone::Battlefield);
        let donor = put(&mut state, actor, "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(donor, Zone::Exile),
        );
        let generation = state.objects.get(banishing).zone_change_count;
        state
            .attach_object_exact(attachment, 0, banishing, generation)
            .unwrap();
        let source = AbilitySourceContractV4::capture(&state, attachment);
        state.objects.get_mut(donor).v4.exiled_by = Some(ObjectLinkV4 {
            object: attachment,
            zone_change_count: source.zone_change_count,
        });
        let mut ctx = ExecCtx::no_targets(attachment, actor);
        ctx.ability_source_contract = Some(source);
        let timestamp = state.objects.get(attachment).v4.layer_timestamp.unwrap();
        let chosen = EffectObjectBinding {
            object: donor,
            expected_zone: Zone::Exile,
            expected_zone_change_count: state.objects.get(donor).zone_change_count,
        };
        let link = ObjectLinkV4 {
            object: banishing,
            zone_change_count: generation,
        };
        assert!(
            crate::standard_cards_v1::aegis_copy_candidates(&state, &ctx, link, timestamp)
                .contains(&chosen)
        );
        crate::standard_cards_v1::apply_aegis_copy(&mut state, &ctx, link, timestamp, chosen);
        // Real accepted typed copy setup, not a printed Longbow/Aegis spell.
        let hunter = put(&mut state, actor, "High-Society Hunter", Zone::Battlefield);
        put(&mut state, actor, "Forest", Zone::Hand);
        state.step = crate::state::Step::DeclareAttackers;
        crate::engine::step(
            &mut state,
            crate::engine::Action::DeclareAttackers(vec![hunter]),
        )
        .unwrap();
        for _ in 0..16 {
            let decision = crate::engine::advance_until_decision(&mut state);
            if matches!(
                decision,
                crate::engine::Decision::ChooseEffectTargets { .. }
            ) {
                break;
            }
            assert!(
                matches!(decision, crate::engine::Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            crate::engine::step(&mut state, crate::engine::Action::Pass).unwrap();
        }
        crate::engine::step(
            &mut state,
            crate::engine::Action::ChooseEffectTarget(Target::Object(banishing)),
        )
        .unwrap();
        assert!(
            matches!(crate::engine::advance_until_decision(&mut state),crate::engine::Decision::ChooseEffectTargets{player,source,..}if player==actor.opponent()&&source==aura)
        );
        let mut session = HumanHunterSessionV45::from_session(
            FastActorSessionV1::from_hunter_v45_fixture_state(state),
            actor.opponent().into(),
        )
        .unwrap();
        let mut projector = HumanHunterProjectorV45::new(actor.opponent().into());
        let e = expected(&session);
        let public = session.public_observation(e).unwrap();
        assert!(public.hunter.menu.is_none());
        let row = public
            .projection
            .surface
            .engine_context
            .pending_effect
            .as_ref()
            .unwrap();
        assert_eq!(row.source.as_ref().unwrap().arena_id, aura.0);
        assert_eq!(public.hunter.occurrences[0].source.arena_id, hunter.0);
        let menu = projector.project_current(&session, e).unwrap();
        assert!(menu
            .actions
            .iter()
            .any(|row| row.label.contains("Twinblade Blessing")
                && row.label.contains("Llanowar Elves")));
        assert!(menu
            .actions
            .iter()
            .all(|row| !row.label.contains("sacrifice")));
        let saved = session.snapshot_v45();
        let action = menu
            .actions
            .iter()
            .find(|row| row.label.contains("Llanowar Elves"))
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: menu.prompt_seq,
            action_index: action.action_index,
        };
        projector.submit(&mut session, request.clone()).unwrap();
        session.restore_v45(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        submit(&mut session, &mut projector, |label| {
            label.contains("Llanowar Elves")
        });
        for _ in 0..30 {
            if session.inner.hunter_fixture_state().stack.is_empty()
                && session
                    .inner
                    .hunter_fixture_state()
                    .engine
                    .pending_triggers
                    .is_empty()
            {
                break;
            }
            pass(&mut session, &mut projector, actor.opponent());
        }
        let state = session.inner.hunter_fixture_state();
        assert_eq!(state.objects.get(hunter).counters.plus1_plus1, 1);
        assert_eq!(
            state.objects.get(aura).v4.attached_to,
            Some(ObjectLinkV4 {
                object: host,
                zone_change_count: state.objects.get(host).zone_change_count
            })
        );
        assert!(!crate::linked_aura_operation_v1::has_pending(state));
    }
}
