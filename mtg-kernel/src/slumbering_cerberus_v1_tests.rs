//! Registered operands exercise typed source/untap primitives, not printed Cerberus.
use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::state::AbilitySourceContractV4;

fn ready(actor: PlayerId) -> (GameState, ObjectId, ExecCtx) {
    let elf = card_id_by_name("Llanowar Elves").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[elf; 40],
        &[elf; 40],
        |id| CARD_DEFS[id as usize].object_name.into(),
        110_094,
        actor,
    );
    let source = state.players[actor.index()].library[0];
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(source, Zone::Battlefield),
    );
    state.objects.get_mut(source).tapped = true;
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, source));
    (state, source, ctx)
}
fn restore(state: &GameState) -> GameState {
    serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
}
#[test]
fn captured_untap_uses_stun_replacement_and_replays_exactly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, ctx) = ready(actor);
        assert!(!prevents_untap_step(&state, source));
        state.objects.get_mut(source).counters.stun = 1;
        state.objects.get_mut(source).v4.skip_next_untap = true;
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            untap_captured_source(&ctx, branch);
            assert!(branch.objects.get(source).tapped);
            assert_eq!(branch.objects.get(source).counters.stun, 0);
            assert!(branch.objects.get(source).v4.skip_next_untap);
            untap_captured_source(&ctx, branch);
            assert!(!branch.objects.get(source).tapped);
            assert!(branch.objects.get(source).v4.skip_next_untap);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn captured_untap_survives_control_change_but_never_follows_reentry() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let (mut state, source, ctx) = ready(actor);
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
                state.objects.get_mut(source).tapped = true;
            } else {
                // Typed leaf setup isolates current control from captured controller.
                state.objects.get_mut(source).controller = actor.opponent();
            }
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                untap_captured_source(&ctx, branch);
                assert_eq!(branch.objects.get(source).tapped, reenter);
                assert!(branch.engine.halted.is_none());
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn malformed_captured_authority_halts_before_untap() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..4 {
            let (mut state, source, mut ctx) = ready(actor);
            match case {
                0 => ctx.ability_source_contract = None,
                1 => ctx.ability_source_contract.as_mut().unwrap().owner = actor.opponent(),
                2 => ctx.ability_source_contract.as_mut().unwrap().controller = actor.opponent(),
                3 => {
                    ctx.ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                _ => unreachable!(),
            }
            state = restore(&state);
            untap_captured_source(&ctx, &mut state);
            assert!(state.engine.halted.is_some());
            assert!(state.objects.get(source).tapped);
        }
    }
}
#[test]
fn absent_printed_source_restriction_is_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source, _) = ready(actor);
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(!prevents_untap_step(&state, source));
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn shared_morbid_uses_actual_death_and_rechecks_round_plus_active_seat() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for dead_owner in [actor, actor.opponent()] {
            let (mut state, source, ctx) = ready(actor);
            let elf = state.players[dead_owner.index()].library[1];
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(elf, Zone::Battlefield),
            );
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(elf, Zone::Graveyard));
            assert!(state.creature_died_this_turn_v1());
            let mut replay = restore(&state);
            for branch in [&mut state, &mut replay] {
                crate::effect::execute(&program_v1(), &ctx, branch);
                assert!(!branch.objects.get(source).tapped);
                branch.objects.get_mut(source).tapped = true;
                branch.active_player = actor.opponent();
                assert!(!branch.creature_died_this_turn_v1());
                crate::effect::execute(&program_v1(), &ctx, branch);
                assert!(branch.objects.get(source).tapped);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
