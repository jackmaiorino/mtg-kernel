use super::*;
use crate::{
    ids::ObjectId,
    mana::ManaColor,
    rl::PublicObservationProjectionV2,
    state::{StackItemKind, Step},
};

fn board(decision: &KaitoDecision) -> &PublicObservationProjectionV2 {
    &decision.fixture_backend_v75().observation().common.common.projection.surface
}

fn action_index(decision: &KaitoDecision, choose: impl Fn(&Action) -> bool) -> Option<u32> {
    decision
        .fixture_backend_v75()
        .actions()
        .iter()
        .position(choose)
        .map(|i| u32::try_from(i).unwrap())
}

fn apply(owner: &mut KaitoSession, native: &mut FastActorSessionV1, d: &KaitoDecision, i: u32) {
    let backend = d.fixture_backend_v75();
    let expected = backend.expected();
    let original = backend.fixture_original_index_v75(i).unwrap();
    assert_eq!(
        owner.apply(d, i).unwrap(),
        native.step(expected.episode_id, expected.step, original).unwrap()
    );
    assert_eq!(owner.fixture_state(), native.kaito_fixture_state());
}

fn act(owner: &mut KaitoSession, native: &mut FastActorSessionV1, choose: impl Fn(&Action) -> bool) {
    let d = decision(owner);
    let i = action_index(&d, choose)
        .unwrap_or_else(|| panic!("expected action in {:?}", d.fixture_backend_v75().actions()));
    apply(owner, native, &d, i);
}

fn pass(action: &Action) -> bool {
    matches!(action, Action::Pass { .. })
}

fn settle(owner: &mut KaitoSession, native: &mut FastActorSessionV1) {
    for _ in 0..24 {
        if owner.fixture_state().stack.is_empty() {
            return;
        }
        act(owner, native, pass);
    }
    panic!("paid spell or attack trigger did not finish");
}

fn ordinary_progress(action: &Action) -> bool {
    matches!(
        action,
        Action::Pass { .. }
            | Action::ChooseAttackerInclusion { include: false, .. }
            | Action::ChooseBlockerInclusion { include: false, .. }
    ) || matches!(action, Action::DeclareAttackers { attackers, .. } if attackers.is_empty())
        || matches!(action, Action::DeclareBlockersForAttacker { blockers, .. } if blockers.is_empty())
}

fn tap_action(action: &Action, object: ObjectId) -> bool {
    matches!(action, Action::ActivateManaAbility { source, .. } if source.arena_id == object.0)
}

fn cast_action(action: &Action, object: ObjectId) -> bool {
    matches!(action, Action::CastSpell { source, .. } if source.arena_id == object.0)
}

fn advance_to_actor_main(owner: &mut KaitoSession, native: &mut FastActorSessionV1, actor: PlayerId) {
    let turn = owner.fixture_state().turn_key_v1();
    for _ in 0..96 {
        let state = owner.fixture_state();
        if state.turn_key_v1() != turn
            && state.active_player == actor
            && state.priority_player == actor
            && state.step == Step::Main1
        {
            return;
        }
        act(owner, native, ordinary_progress);
    }
    panic!("next main phase did not become a real actor decision");
}

fn assert_stats(d: &KaitoDecision, object: ObjectId, power: i32, toughness: i32) {
    let live = board(d).battlefield.iter().flatten()
        .find(|card| card.stable.arena_id == object.0)
        .unwrap();
    assert_eq!(live.characteristics.effective_power, Some(power));
    assert_eq!(live.characteristics.effective_toughness, Some(toughness));
}

#[test]
fn paid_elf_lords_keep_original_actions_and_live_counts_for_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, mana, power, toughness) in [
            ("Dwynen, Gilt-Leaf Daen", 4, 3, 4),
            ("Elvish Archdruid", 3, 2, 2),
        ] {
            let library = [card("Forest"); 40];
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &library, &library, crate::rl::card_name, 550_210, actor,
            );
            state.step = Step::Main1;
            crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
            FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
            let lands: Vec<_> = (0..mana)
                .map(|_| put(&mut state, actor, "Forest", Zone::Battlefield))
                .collect();
            let own_elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let other_elf = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Battlefield);
            let hawk = put(&mut state, actor, "Healer's Hawk", Zone::Battlefield);
            let lord = put(&mut state, actor, name, Zone::Hand);
            let removal = put(&mut state, actor.opponent(), "Stab", Zone::Hand);
            let swamp = put(&mut state, actor.opponent(), "Swamp", Zone::Battlefield);
            let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
            let mut native = raw(&state);
            for land in &lands {
                act(&mut owner, &mut native, |action| tap_action(action, *land));
            }
            let d = decision(&owner);
            assert_model(&owner, &d);
            let cast = action_index(&d, |action| cast_action(action, lord)).unwrap();
            apply(&mut owner, &mut native, &d, cast);
            assert_eq!(owner.fixture_state().objects.get(lord).zone, Zone::Stack);
            assert!(lands.iter().all(|&id| owner.fixture_state().objects.get(id).tapped));
            assert!(!owner.fixture_state().objects.get(own_elf).tapped);
            assert_eq!(owner.fixture_state().players[actor.index()].mana_pool, [0; 6]);
            settle(&mut owner, &mut native);
            assert!(owner.fixture_state().objects.get(lord).summoning_sick);
            let entered = decision(&owner);
            assert_model(&owner, &entered);
            assert_stats(&entered, lord, power, toughness);
            assert_stats(&entered, own_elf, 2, 2);
            assert_stats(&entered, other_elf, 1, 1);
            assert_stats(&entered, hawk, 1, 1);
            assert!(action_index(&entered, |action| tap_action(action, lord)).is_none());
            advance_to_actor_main(&mut owner, &mut native, actor);

            if name == "Elvish Archdruid" {
                let ready = decision(&owner);
                assert_model(&owner, &ready);
                let tap = action_index(&ready, |action| tap_action(action, lord)).unwrap();
                let saved = owner.snapshot();
                let native_saved = native.snapshot_v1();
                let green = ManaColor::G.pool_index();
                let before = board(&ready).mana_pools[actor.index()][green];
                for _ in 0..2 {
                    let current = decision(&owner);
                    assert_eq!(current.packet(), ready.packet());
                    apply(&mut owner, &mut native, &current, tap);
                    let live = owner.fixture_state();
                    assert!(live.stack.is_empty());
                    assert!(live.objects.get(lord).tapped);
                    assert_eq!(live.players[actor.index()].mana_pool[green], before + 2);
                    owner.restore(&saved).unwrap();
                    native.kaito_restore_v75(&native_saved).unwrap();
                    assert!(owner.apply(&current, tap).is_err());
                }
                continue;
            }

            for _ in 0..24 {
                if owner.fixture_state().stack.iter().any(|item| {
                    item.source == lord && item.kind == StackItemKind::TriggeredAbility
                }) {
                    break;
                }
                let d = decision(&owner);
                let attack = action_index(&d, |action| match action {
                    Action::ChooseAttackerInclusion { attacker, include, .. } =>
                        *include == [lord.0, own_elf.0].contains(&attacker.arena_id),
                    Action::DeclareAttackers { attackers, .. } =>
                        attackers.len() == 2 && attackers.iter().all(|a| [lord.0, own_elf.0].contains(&a.arena_id)),
                    _ => false,
                }).or_else(|| action_index(&d, pass)).expect("actual attack declaration");
                apply(&mut owner, &mut native, &d, attack);
            }
            let triggered = decision(&owner);
            assert_model(&owner, &triggered);
            assert_eq!(board(&triggered).combat.ordered_attackers.len(), 2);
            assert!(owner.fixture_state().stack.iter().any(|item| {
                item.source == lord && item.kind == StackItemKind::TriggeredAbility
            }));
            let before = board(&triggered).life_totals[actor.index()];
            if action_index(&triggered, |action| cast_action(action, removal)).is_none() {
                act(&mut owner, &mut native, pass);
            }
            act(&mut owner, &mut native, |action| cast_action(action, removal));
            act(&mut owner, &mut native, |action| {
                matches!(action,
                    Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                    | Action::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
                        if object.arena_id == own_elf.0)
            });
            let after_target = decision(&owner);
            if let Some(finish) = action_index(&after_target, |action| {
                matches!(action, Action::FinishTargetSelection { .. } | Action::FinishEffectSelection { .. })
            }) {
                apply(&mut owner, &mut native, &after_target, finish);
            }
            assert!(owner.fixture_state().objects.get(swamp).tapped);
            settle(&mut owner, &mut native);
            let live = owner.fixture_state();
            assert_eq!(live.objects.get(own_elf).zone, Zone::Graveyard);
            assert_eq!(live.players[actor.index()].life, before + 1);
        }
    }
}
