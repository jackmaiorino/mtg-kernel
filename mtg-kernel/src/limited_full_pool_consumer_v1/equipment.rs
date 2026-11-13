//! Paid numeric actions preserve attachment targets and temporary bonuses.
use super::elf_lords::{act, action_index, advance_to_actor_main, apply, assert_stats, pass};
use super::*;
use crate::{card_def::Keywords, ids::ObjectId, state::Step};

fn settle_to(
    owner: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    target: ObjectId,
    done: impl Fn(&GameState) -> bool,
) {
    for _ in 0..32 {
        let state = owner.fixture_state();
        if state.stack.is_empty() && done(state) {
            return;
        }
        let d = decision(owner);
        assert_model(owner, &d);
        let selected = action_index(&d, |action| {
            matches!(action,
            Action::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                if object.arena_id == target.0)
        })
        .or_else(|| {
            action_index(&d, |action| {
                matches!(
                    action,
                    Action::FinishTargetSelection { .. } | Action::FinishEffectSelection { .. }
                )
            })
        })
        .or_else(|| action_index(&d, pass))
        .expect("real equipment continuation");
        apply(owner, native, &d, selected);
    }
    panic!("equipment did not settle to its actual attachment or trigger result");
}

fn attached(state: &GameState, equipment: ObjectId, creature: ObjectId) -> bool {
    state
        .objects
        .get(equipment)
        .v4
        .attached_to
        .is_some_and(|link| {
            link.object == creature
                && link.zone_change_count == state.objects.get(creature).zone_change_count
        })
}

fn equip(
    owner: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    equipment: ObjectId,
    target: ObjectId,
) {
    act(owner, native, |action| {
        matches!(action,
        Action::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == equipment.0)
    });
    settle_to(owner, native, target, |s| attached(s, equipment, target));
}

#[test]
fn paid_equipment_entry_landfall_reattachment_and_cleanup_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (name, cast_cost, equip_cost) in [("Celestial Armor", 3, 4), ("Adventuring Gear", 1, 1)]
        {
            let armor = name == "Celestial Armor";
            let library = [card("Forest"); 40];
            let mut state = GameState::new_from_libraries_with_starting_player_v1(
                &library,
                &library,
                crate::rl::card_name,
                550_234,
                actor,
            );
            state.step = Step::Main1;
            FastActorSessionV1::enable_limited_native_modes_v1(
                &mut state,
                LimitedAppendixV1::FullPool,
            );
            let lands: Vec<_> = (0..8)
                .map(|_| put(&mut state, actor, "Plains", Zone::Battlefield))
                .collect();
            // These two ordinary creatures are initial attachment targets.
            let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let second = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let equipment = put(&mut state, actor, name, Zone::Hand);
            let land = put(&mut state, actor, "Forest", Zone::Hand);
            let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
            let mut native = raw(&state);
            for &land in lands.iter().take(cast_cost) {
                act(&mut owner, &mut native, |action| {
                    matches!(action,
                    Action::ActivateManaAbility { source, .. } if source.arena_id == land.0)
                });
            }
            act(&mut owner, &mut native, |action| {
                matches!(action,
                Action::CastSpell { source, .. } if source.arena_id == equipment.0)
            });
            assert_eq!(
                owner.fixture_state().players[actor.index()].mana_pool,
                [0; 6]
            );
            settle_to(&mut owner, &mut native, first, |s| {
                s.objects.get(equipment).zone == Zone::Battlefield
                    && (!armor || attached(s, equipment, first))
            });
            let tapped =
                |s: &GameState| lands.iter().filter(|&&id| s.objects.get(id).tapped).count();
            assert_eq!(tapped(owner.fixture_state()), cast_cost);
            if armor {
                assert_stats(&decision(&owner), first, 3, 1);
                for keyword in [
                    Keywords::FLYING,
                    Keywords::HEXPROOF,
                    Keywords::INDESTRUCTIBLE,
                ] {
                    assert!(crate::engine::has_effective_keyword(
                        owner.fixture_state(),
                        first,
                        keyword
                    ));
                }
            } else {
                equip(&mut owner, &mut native, equipment, first);
                assert_eq!(tapped(owner.fixture_state()), cast_cost + equip_cost);
                let saved = owner.snapshot();
                let native_saved = native.snapshot_v1();
                let before = decision(&owner);
                let play = action_index(&before, |action| {
                    matches!(action,
                    Action::PlayLand { source, .. } if source.arena_id == land.0)
                })
                .unwrap();
                apply(&mut owner, &mut native, &before, play);
                settle_to(&mut owner, &mut native, first, |s| {
                    crate::engine::effective_power(s, first) == 3
                });
                assert_stats(&decision(&owner), first, 3, 3);
                owner.restore(&saved).unwrap();
                native.kaito_restore_v75(&native_saved).unwrap();
                assert!(owner.apply(&before, play).is_err());
                let restored = decision(&owner);
                assert_eq!(restored.packet(), before.packet());
                apply(&mut owner, &mut native, &restored, play);
                settle_to(&mut owner, &mut native, first, |s| {
                    crate::engine::effective_power(s, first) == 3
                });
            }
            equip(&mut owner, &mut native, equipment, second);
            assert_eq!(
                tapped(owner.fixture_state()),
                cast_cost + equip_cost * if armor { 1 } else { 2 }
            );
            let d = decision(&owner);
            assert_model(&owner, &d);
            assert_stats(
                &d,
                first,
                if armor { 1 } else { 3 },
                if armor { 1 } else { 3 },
            );
            assert_stats(&d, second, if armor { 3 } else { 1 }, 1);
            assert!(!crate::engine::has_effective_keyword(
                owner.fixture_state(),
                second,
                Keywords::HEXPROOF
            ));
            advance_to_actor_main(&mut owner, &mut native, actor);
            assert_stats(&decision(&owner), first, 1, 1);
            for keyword in [Keywords::HEXPROOF, Keywords::INDESTRUCTIBLE] {
                assert!(!crate::engine::has_effective_keyword(
                    owner.fixture_state(),
                    first,
                    keyword
                ));
            }
        }
    }
}
