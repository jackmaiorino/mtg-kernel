use super::*;
use crate::policy_observation_v6::tests::ready_state;

#[test]
fn registered_absence_retains_predecessor_public_private_and_ward_fields_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = ready_state();
        let surface = PolicySurfaceV5::new_for_session();
        let before = serde_json::to_vec(&state).unwrap();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let new = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        assert_eq!(new.projection, old.projection);
        assert_eq!(new.own_hand, old.own_hand);
        assert_eq!(new.known_library_cards, old.known_library_cards);
        assert_eq!(new.known_hand_cards, old.known_hand_cards);
        assert_eq!(new.known_face_down_cards, old.known_face_down_cards);
        assert_eq!(new.extensions, old.extensions);
        assert!(new.choice.is_none());
        new.validate_artifact().unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod registered {
    use super::*;
    use crate::card_def::{card_id_by_name, CARD_DEFS};
    use crate::engine::{self, Action, Decision};
    use crate::policy_observation_v6::tests::put;
    use crate::state::Target;

    pub(crate) fn at_mill(actor: PlayerId, hidden: &str) -> GameState {
        at_mill_case(actor, hidden, false, false)
    }
    fn at_mill_case(actor: PlayerId, hidden: &str, empty: bool, flashback: bool) -> GameState {
        let mut defs = [
            "Lightning Bolt",
            "Ponder",
            "Forest",
            "Forest",
            "Forest",
            "Forest",
        ]
        .map(|n| card_id_by_name(n).unwrap());
        if empty {
            defs.fill(card_id_by_name("Forest").unwrap());
        }
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &defs,
            &defs,
            |id| CARD_DEFS[id as usize].name.into(),
            11037,
            actor,
        );
        state.step = crate::state::Step::Main1;
        put(&mut state, actor.opponent(), hidden, Zone::Hand);
        if !empty {
            put(&mut state, actor, "Counterspell", Zone::Graveyard);
        }
        let spell = put(
            &mut state,
            actor,
            "Inspiration from Beyond",
            if flashback {
                Zone::Graveyard
            } else {
                Zone::Hand
            },
        );
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] =
            if flashback { 5 } else { 2 };
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::U.pool_index()] =
            if flashback { 2 } else { 1 };
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        for _ in 0..16 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(
                state
                    .engine
                    .pending_effect
                    .as_ref()
                    .and_then(|p| p.choice.as_ref()),
                Some(PendingEffectChoice::SelectTargets {
                    purpose: EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
                    ..
                })
            ) {
                assert!(
                    matches!(decision,Decision::ChooseEffectTargets{player,..} if player==actor)
                );
                return state;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        panic!("genuine private mill choice not reached")
    }

    #[test]
    fn genuine_flashback_empty_eligible_finish_keeps_root_and_exiles_after_resolution_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = at_mill_case(actor, "Island", true, true);
            let spell = state.engine.pending_effect.as_ref().unwrap().ctx.source;
            for _ in 0..8 {
                let decision = engine::advance_until_decision(&mut state);
                let row = current_choice(&state, actor).unwrap().unwrap();
                assert_eq!(row.cast_root, CastRootV1::Flashback);
                if row.phase == PhaseV1::ReturnCard {
                    break;
                }
                let Decision::ChooseEffectTargets { legal_targets, .. } = decision else {
                    panic!("actual mill: {decision:?}")
                };
                engine::step(&mut state, Action::ChooseEffectTarget(legal_targets[0])).unwrap();
            }
            let row = current_choice(&state, actor).unwrap().unwrap();
            assert_eq!(row.phase, PhaseV1::ReturnCard);
            assert_eq!((row.minimum, row.maximum), (0, 0));
            assert!(row.legal.is_empty() && row.can_finish);
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            for branch in [&mut state, &mut restored] {
                engine::step(branch, Action::FinishEffectSelection).unwrap();
                assert!(matches!(
                    engine::advance_until_decision(branch),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_eq!(branch.objects.get(spell).zone, Zone::Exile);
                assert!(branch.engine.pending_effect.is_none());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }

    #[test]
    fn real_unknown_mill_scope_and_mandatory_return_are_owned_restorable_and_not_legacy_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = at_mill(actor, "Lightning Bolt");
            let surface = PolicySurfaceV5::new_for_session();
            let bytes = serde_json::to_vec(&state).unwrap();
            assert!(state.known_library_cards(actor, actor).is_empty());
            assert!(observe_v1(&state, &surface, actor.opponent(), 0, 0, 0, 1).is_err());
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            assert!(crate::rl::observe_v2(
                &state,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor,
                0
            )
            .is_err());
            let first = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            let choice = first.choice.as_ref().unwrap();
            assert_eq!(choice.phase, PhaseV1::MillOrder);
            assert_eq!((choice.minimum, choice.maximum), (3, 3));
            assert_eq!(choice.legal.len(), 3);
            assert!(choice.selected.is_empty() && !choice.can_finish);
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                observe_v1(&restored, &surface, actor, 0, 0, 0, 1).unwrap(),
                first
            );
            for branch in [&mut state, &mut restored] {
                for _ in 0..8 {
                    let d = engine::advance_until_decision(branch);
                    if let Some(c) = current_choice(branch, actor).unwrap() {
                        if c.phase == PhaseV1::ReturnCard {
                            break;
                        }
                    }
                    let Decision::ChooseEffectTargets { legal_targets, .. } = d else {
                        panic!("actual mill ordering: {d:?}")
                    };
                    engine::step(branch, Action::ChooseEffectTarget(legal_targets[0])).unwrap();
                }
                let c = current_choice(branch, actor).unwrap().unwrap();
                assert_eq!(c.phase, PhaseV1::ReturnCard);
                assert_eq!((c.minimum, c.maximum), (1, 1));
                assert!(!c.can_finish && !c.ordered);
                assert!(c
                    .legal
                    .iter()
                    .all(|r| r.zone == Zone::Graveyard && r.owner == actor.into()));
                assert_eq!(
                    c.legal.len(),
                    3,
                    "two milled spell cards and one existing Counterspell"
                );
                let before = serde_json::to_vec(branch).unwrap();
                assert!(engine::step(branch, Action::FinishEffectSelection).is_err());
                assert_eq!(serde_json::to_vec(branch).unwrap(), before);
                engine::step(
                    branch,
                    Action::ChooseEffectTarget(Target::Object(ObjectId(c.legal[0].arena_id))),
                )
                .unwrap();
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            for mode in 0..4 {
                let mut bad = at_mill(actor, "Island");
                let candidate = bad.players[actor.index()].library[0];
                match mode {
                    0 => bad.objects.get_mut(candidate).zone_change_count += 1,
                    1 => bad.objects.get_mut(candidate).owner = actor.opponent(),
                    2 => bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            crate::event::CommittedEvent::InspirationResolutionBoundaryV1 { .. }
                        )
                    }),
                    _ => bad.engine.event_history.retain(|e| {
                        !matches!(
                            e,
                            crate::event::CommittedEvent::InspirationResolutionInputV1 { .. }
                        )
                    }),
                }
                let bytes = serde_json::to_vec(&bad).unwrap();
                assert!(observe_v1(&bad, &surface, actor, 0, 0, 0, 1).is_err());
                assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            }
        }
    }
}
