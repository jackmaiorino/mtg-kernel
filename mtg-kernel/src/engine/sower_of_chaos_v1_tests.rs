//! Registered typed operands test the restriction substrate, not Sower admission.
use super::*;
use crate::effect::{self, ExecCtx};
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::sower_of_chaos_v1;
use crate::state::{AbilitySourceContractV4, StackTargetContractV4};

fn physical(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId, ExecCtx) {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let source = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
    let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
    let target = put(
        &mut state,
        actor.opponent(),
        "Guttersnipe",
        Zone::Battlefield,
    );
    let mut ctx = ExecCtx::no_targets(source, actor);
    ctx.targets = vec![Target::Object(target)];
    ctx.target_contracts = vec![StackTargetContractV4::capture(
        &state,
        Target::Object(target),
    )];
    ctx.ability_source_contract = Some(AbilitySourceContractV4::capture(&state, source));
    (state, source, attacker, target, ctx)
}
#[test]
fn exact_rule_restriction_blocks_real_declaration_and_replays_without_becoming_a_keyword() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, attacker, target, ctx) = physical(actor);
        state.step = Step::DeclareAttackers;
        step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        state.step = Step::DeclareBlockers;
        assert!(legal_blockers_for(&state, attacker).contains(&target));
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut replay: GameState = serde_json::from_slice(&bytes).unwrap();
        let opcode = effect::EffectOp::PreventTargetBlockingUntilEndOfTurn { target_index: 0 };
        effect::execute(&opcode, &ctx, &mut state);
        effect::execute(&opcode, &ctx, &mut replay);
        assert!(sower_of_chaos_v1::cant_block(&state, target));
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
        assert_eq!(effective_power(&state, target), 2);
        assert!(!legal_blockers_for(&state, attacker).contains(&target));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(step(
            &mut state,
            Action::DeclareBlockers(vec![(target, attacker)])
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        // No blocker keyword or static printed-source mutation represents it.
        assert_eq!(
            state.objects.get(target).v4.effective_color_mask,
            crate::card_def::mana_color_mask(crate::mana::ManaColor::R)
        );
    }
}
#[test]
fn source_departure_control_changes_power_changes_and_target_reentry_obey_exact_binding() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source, _, target, ctx) = physical(actor);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(source, Zone::Graveyard),
        );
        state.objects.get_mut(target).counters.plus1_plus1 = 20;
        sower_of_chaos_v1::install(&mut state, &ctx, 0);
        assert!(sower_of_chaos_v1::cant_block(&state, target));
        state.players[actor.opponent().index()]
            .battlefield
            .retain(|&id| id != target);
        state.players[actor.index()].battlefield.push(target);
        state.objects.get_mut(target).controller = actor;
        assert!(sower_of_chaos_v1::cant_block(&state, target));
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Graveyard),
        );
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(target, Zone::Battlefield),
        );
        assert!(!sower_of_chaos_v1::cant_block(&state, target));
        let before = serde_json::to_vec(&state).unwrap();
        sower_of_chaos_v1::install(&mut state, &ctx, 0);
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    }
}
#[test]
fn typed_target_rechecks_refuse_hexproof_protection_and_forged_incarnations_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for case in 0..3 {
            let (mut state, _, _, target, mut ctx) = physical(actor);
            match case {
                0 | 1 => {
                    let timestamp = next_timestamp(&mut state);
                    state.engine.until_end_of_turn.push(
                        UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
                            object_id: target,
                            object_zone_change_count: state.objects.get(target).zone_change_count,
                            layer: Layers::ABILITY_ADDING,
                            timestamp,
                            duration: EffectDuration::EndOfTurn,
                            keywords: if case == 0 {
                                Keywords::HEXPROOF
                            } else {
                                Keywords::PROTECTION_FROM_MONOCOLORED
                            },
                        },
                    );
                }
                _ => ctx.target_contracts = vec![StackTargetContractV4::Player(actor)],
            }
            let before = serde_json::to_vec(&state).unwrap();
            sower_of_chaos_v1::install(&mut state, &ctx, 0);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            assert!(!sower_of_chaos_v1::cant_block(&state, target));
        }
    }
}
#[test]
fn cleanup_and_real_repeated_cleanup_boundaries_clear_only_new_records_without_legacy_bytes() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, _, _, target, ctx) = physical(actor);
        state.step = Step::Cleanup;
        run_step_entry_action(&mut state, Step::Cleanup);
        let absent = serde_json::to_vec(&state).unwrap();
        let old: GameState = serde_json::from_slice(&absent).unwrap();
        assert_eq!(serde_json::to_vec(&old).unwrap(), absent);
        sower_of_chaos_v1::install(&mut state, &ctx, 0);
        assert!(sower_of_chaos_v1::cant_block(&state, target));
        // Surplus-pool boundary operands, not a printed restricted-mana producer.
        for player in &mut state.players {
            player.mana_pool = [1, 2, 3, 4, 5, 6];
            player
                .restricted_mana_pool
                .0
                .push(crate::mana::RestrictedManaUnitV1 {
                    color: crate::mana::ManaColor::R,
                    restriction:
                        crate::card_def::ManaSpendRestrictionDef::CreatureSpellOrCreatureAbility,
                    source: crate::state::ObjectLinkV4 {
                        object: ctx.source,
                        zone_change_count: 0,
                    },
                    source_card_def: state.objects.get(ctx.source).card_def,
                });
        }
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        advance_step(&mut state);
        advance_step(&mut replay);
        assert_eq!(state.step, Step::Cleanup);
        assert_eq!(state.active_player, actor);
        assert!(state.engine.until_end_of_turn.is_empty());
        for player in &state.players {
            assert_eq!(player.mana_pool, [0; 6]);
            assert!(player.restricted_mana_pool.is_empty());
        }
        assert!(!sower_of_chaos_v1::cant_block(&state, target));
        advance_step(&mut state);
        advance_step(&mut replay);
        assert_eq!(state.active_player, actor.opponent());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&replay).unwrap()
        );
    }
}

#[test]
fn prepared_devil_preserves_accepted_subtype_prefix_and_delays_choice_admission() {
    use crate::card_def::Subtype;
    assert_eq!(Subtype::Orc.stable_id(), 146);
    assert_eq!(Subtype::Juggernaut.stable_id(), 147);
    assert_eq!(Subtype::Minotaur.stable_id(), 148);
    assert_eq!(Subtype::Ooze.stable_id(), 149);
    assert_eq!(Subtype::Devil.stable_id(), 150);
    assert!(Subtype::Devil.is_creature_type());
    assert!(!Subtype::ALL.contains(&Subtype::Devil));
    assert!(!Subtype::CREATURE_TYPES.contains(&Subtype::Devil));
}
