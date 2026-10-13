//! Registered operands exercise the persistent source-upgrade substrate only.
use crate::card_def::Subtype;
use crate::effect::{self, ExecCtx};
use crate::event::{self, ProposedEvent};
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::put;
use crate::state::{AbilitySourceContractV4, GameState, Zone};

fn physical(actor: PlayerId) -> (GameState, crate::ids::ObjectId, ExecCtx) {
    let forest = crate::card_def::card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 12],
        &[forest; 12],
        |_| "Forest".into(),
        145,
        actor,
    );
    let source = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, source));
    (state, source, ctx)
}
#[test]
fn exact_upgrade_uses_existing_type_timestamp_and_counter_event_for_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, ctx) = physical(actor);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            let hand = branch.players[actor.index()].hand.len();
            effect::execute(&super::program_v1(), &ctx, branch);
            assert_eq!(branch.objects.get(source).counters.plus1_plus1, 2);
            assert_eq!(crate::engine::effective_power(branch, source), 3);
            assert_eq!(
                crate::engine::effective_subtype_ids(branch, source),
                vec![Subtype::Werewolf.stable_id()]
            );
            assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
        }
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}
#[test]
fn departed_or_reentered_source_receives_no_upgrade_but_captured_controller_draws() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for reenter in [false, true] {
            let (mut state, source, ctx) = physical(actor);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            if reenter {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(source, Zone::Battlefield),
                );
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                let hand = branch.players[actor.index()].hand.len();
                effect::execute(&super::program_v1(), &ctx, branch);
                assert_eq!(branch.objects.get(source).counters.plus1_plus1, 0);
                assert!(branch.objects.get(source).v4.creature_upgrade.is_none());
                assert_eq!(branch.players[actor.index()].hand.len(), hand + 1);
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
#[test]
fn counter_type_instruction_requires_source_owner_controller_and_incarnation_binding() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for field in 0..4 {
            let (mut state, _, mut ctx) = physical(actor);
            let mut contract = ctx.ability_source_contract.unwrap();
            match field {
                0 => contract.owner = actor.opponent(),
                1 => contract.controller = actor.opponent(),
                2 => contract.zone_change_count += 1,
                _ => contract.zone = Zone::Graveyard,
            }
            ctx.ability_source_contract = Some(contract);
            let before = serde_json::to_vec(&state).unwrap();
            super::resolve_source_upgrade(&mut state, &ctx);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
}
#[test]
fn absent_registered_librarian_preserves_paid_activation_hook_state_bytes_and_hash() {
    use std::hash::{Hash, Hasher};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, _) = physical(actor);
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut before = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut before);
        assert!(super::activation_allowed(&state, source, 0));
        super::activation_paid(&mut state, source, 0);
        let mut after = std::collections::hash_map::DefaultHasher::new();
        state.hash(&mut after);
        assert_eq!(before.finish(), after.finish());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    }
}
