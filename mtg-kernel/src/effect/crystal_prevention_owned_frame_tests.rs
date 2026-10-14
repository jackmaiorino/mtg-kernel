//! The real selected Incinerating root owns its optional discard remainder.
use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{ActiveReplacement, ReplacementEffectKind};
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{put, ready_state_for};

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn restored(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
fn at_prevention(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId) {
    let mut state = ready_state_for(actor);
    for player in [actor, actor.opponent()] {
        put(&mut state, player, "Forest", Zone::Battlefield);
    }
    let wall = put(
        &mut state,
        actor.opponent(),
        "Crystal Barricade",
        Zone::Battlefield,
    );
    let target = put(
        &mut state,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let discard = put(&mut state, actor, "Forest", Zone::Hand);
    let draw = put(&mut state, actor, "Forest", Zone::Library);
    let spell = put(&mut state, actor, "Incinerating Blast", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    // A declared physical shield competes with the actual printed static.
    // Spell/continuation authority is acquired by the genuine cast below.
    state.engine.next_replacement_id = 1;
    state.engine.active_replacements.push(ActiveReplacement {
        id: 1,
        source: wall,
        kind: ReplacementEffectKind::PreventNextDamage {
            target: Target::Object(target),
            remaining: 1,
        },
    });
    assert!(
        matches!(next(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
    );
    engine::step(&mut state, Action::CastSpell(spell)).unwrap();
    assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
    engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
    assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    for _ in 0..12 {
        let decision = next(&mut state);
        if matches!(decision, Decision::ChooseEffectOption { player, source, .. }
            if player == actor.opponent() && source == spell)
        {
            crate::effect::validate_pending_effect_choice(&state).unwrap();
            return (state, spell, target, discard, draw);
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("actual mixed Incinerating prevention menu not reached");
}
fn barricade(state: &GameState) -> u16 {
    let Some(PendingEffectChoice::ChooseOption {
        purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
        ..
    }) = state
        .engine
        .pending_effect
        .as_ref()
        .unwrap()
        .choice
        .as_ref()
    else {
        panic!("genuine prevention menu");
    };
    u16::try_from(
        choice
            .options
            .iter()
            .position(|option| matches!(option, PreventionV1::Barricade(_)))
            .unwrap(),
    )
    .unwrap()
}
fn refusal(state: &GameState) {
    for mut bad in [state.clone(), restored(state)] {
        let bytes = serde_json::to_vec(&bad).unwrap();
        assert!(crate::effect::validate_pending_effect_choice(&bad).is_err());
        assert!(engine::step(&mut bad, Action::Pass).is_err());
        assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        let mut expected = restored(&bad);
        assert!(matches!(
            engine::advance_until_decision(&mut bad),
            Decision::Halted { .. }
        ));
        expected.engine.halted = bad.engine.halted;
        assert_eq!(bad, expected);
    }
}

#[test]
fn real_optional_discard_frame_reaches_pay_decline_and_restores_both_choices_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let (mut state, spell, target, discard, draw) = at_prevention(actor);
            let pending = state.engine.pending_effect.as_ref().unwrap();
            let [EffectFrame::Program { op, path }] = pending.frames.as_slice() else {
                panic!("exact remaining optional discard frame");
            };
            assert_eq!(path, &[1]);
            assert!(authenticates_legacy_frame(&state, pending, op, path).unwrap());
            // The generic interpreter's old unsupported-leaf contract stays fixed.
            assert!(crate::effect::validate_resumable_program(op).is_err());
            let original = state.engine.event_history.clone();
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                let index = barricade(branch);
                engine::step(branch, Action::ChooseEffectOption(index)).unwrap();
                assert!(matches!(next(branch), Decision::ChooseEffectTargets {
                    player, min_targets: 0, max_targets: 1, can_finish: true, ref legal_targets, ..
                } if player == actor && legal_targets == &[Target::Object(discard)]));
                assert_eq!(branch.objects.get(spell).zone, Zone::Stack);
                crate::effect::validate_pending_effect_choice(branch).unwrap();
            }
            assert_eq!(state, replay);
            let offered = restored(&state);
            for branch in [&mut state, &mut replay] {
                engine::step(
                    branch,
                    if pay {
                        Action::ChooseEffectTarget(Target::Object(discard))
                    } else {
                        Action::FinishEffectSelection
                    },
                )
                .unwrap();
                assert!(matches!(next(branch), Decision::CastSpellOrPass { .. }));
                assert!(branch.engine.pending_effect.is_none());
                assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
                assert_eq!(branch.objects.get(target).damage, 0);
                assert_eq!(
                    branch.objects.get(discard).zone,
                    if pay { Zone::Graveyard } else { Zone::Hand }
                );
                assert_eq!(
                    branch.objects.get(draw).zone,
                    if pay { Zone::Hand } else { Zone::Library }
                );
                assert_eq!(branch.engine.event_history[..original.len()], original);
                assert!(matches!(
                    branch.engine.active_replacements[0].kind,
                    ReplacementEffectKind::PreventNextDamage { remaining: 1, .. }
                ));
            }
            assert_eq!(state, replay);
            let mut replay = restored(&offered);
            engine::step(
                &mut replay,
                if pay {
                    Action::ChooseEffectTarget(Target::Object(discard))
                } else {
                    Action::FinishEffectSelection
                },
            )
            .unwrap();
            next(&mut replay);
            assert_eq!(state, replay);
        }
    }
}

#[test]
fn original_crystal_journals_do_not_authorize_unmarked_foreign_or_rewritten_legacy_frames() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _, _, _) = at_prevention(actor);
        let original = state.engine.event_history.clone();
        for mutation in 0..5 {
            let mut bad = restored(&state);
            let pending = bad.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => {
                    pending.resolving_item.v4.crystal_prevention_started_v1 = Default::default();
                    bad.stack
                        .last_mut()
                        .unwrap()
                        .v4
                        .crystal_prevention_started_v1 = Default::default();
                }
                1 => {
                    pending.ctx.controller = actor.opponent();
                    pending.resolving_item.controller = actor.opponent();
                    bad.stack.last_mut().unwrap().controller = actor.opponent();
                }
                2 => {
                    let EffectFrame::Program { path, .. } = &mut pending.frames[0] else {
                        unreachable!()
                    };
                    *path = vec![0];
                }
                3 | 4 => {
                    let EffectFrame::Program {
                        op: EffectOp::MayPayCostThen { discard, then, .. },
                        ..
                    } = &mut pending.frames[0]
                    else {
                        unreachable!()
                    };
                    if mutation == 3 {
                        *discard = 2;
                    } else {
                        **then = EffectOp::DrawCards {
                            player: PlayerRef::Opponent,
                            count: 1,
                        };
                    }
                }
                _ => unreachable!(),
            }
            assert_eq!(bad.engine.event_history, original);
            refusal(&bad);
        }
    }
}
