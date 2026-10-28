//! Genuine paid native input; direct combat/copy operations are explicitly primitives.
use super::*;
use crate::{
    engine,
    engine::time_stop_v1::tests::{cast, end_turn, finish, ready, ready_hidden, restore},
    event::ProposedEvent,
    ids::PlayerId,
    rl::PlayerSeatV1,
    state::{StackItemKind, Target, Zone},
};
fn facts(s: &GameState, p: PlayerId) -> crate::time_stop_public_v68::FactsV68 {
    validate_state(s).unwrap();
    crate::time_stop_public_v68::capture(s, p).unwrap()
}
#[test]
fn genuine_paid_stack_disposal_complete_rows_restore_and_readonly_forgery_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let lower = cast(&mut s, p, "Think Twice", &[], 2);
        let stop = cast(&mut s, p, "Time Stop", &[], 6);
        let original = s.stack.clone();
        end_turn(&mut s);
        let restored = restore(&s);
        for viewer in [p, p.opponent()] {
            let f = facts(&s, viewer);
            assert_eq!(f, facts(&restored, viewer));
            assert_eq!(f.disposals.len(), 1);
            let d = &f.disposals[0];
            assert_eq!(d.disposed_rows.len(), original.len());
            for (row, native) in d.disposed_rows.iter().zip(&original) {
                assert_eq!(row.original.source.arena_id, native.source.0);
                assert_eq!(
                    row.original.controller,
                    PlayerSeatV1::from(native.controller)
                );
                assert_eq!(row.original.is_copy, native.is_copy);
                assert_eq!(row.original.x_value, native.v4.x_value);
                assert!(crate::engine::time_stop_v1::disposed(&s, native).unwrap());
            }
            assert_eq!(s.objects.get(lower).zone, Zone::Exile);
            assert_eq!(s.objects.get(stop).zone, Zone::Exile);
            let mut bad = f.clone();
            bad.disposals[0].disposed_rows.reverse();
            assert!(crate::time_stop_public_v68::validate(&bad).is_err());
            let mut bad = f.clone();
            bad.disposals[0].ending_spell_position = 0;
            assert!(crate::time_stop_public_v68::validate(&bad).is_err());
        }
        let before = serde_json::to_vec(&s).unwrap();
        assert!(crate::engine::time_stop_v1::refuse_legacy(&s).is_err());
        assert_eq!(serde_json::to_vec(&s).unwrap(), before);
    }
}
#[test]
fn genuine_paid_ability_and_native_virtual_copy_outcomes_are_disposed_not_finished_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let h = cast(&mut s, p, "Drake Hatcher", &[], 2);
        finish(&mut s);
        // Genuine native combat-commit primitive, not declared combat.
        engine::commit_combat_damage_events(
            &mut s,
            vec![ProposedEvent::damage(h, Target::Player(p.opponent()), 1)],
        );
        crate::engine::time_stop_v1::tests::priority(&mut s, p);
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == h && r.kind == StackItemKind::TriggeredAbility));
        let stop = cast(&mut s, p, "Time Stop", &[], 6);
        // Existing native spell-copy primitive creates actual virtual ancestry.
        let (copy, _) = engine::copy_spell_for(&mut s, stop, p).unwrap();
        assert!(s.objects.get(copy).spell_copy_origin.is_some());
        end_turn(&mut s);
        let f = facts(&s, p);
        let rows = &f.disposals[0].disposed_rows;
        assert!(rows
            .iter()
            .any(|r| r.original.is_copy && r.original.source.card_db_id == 486));
        assert!(rows
            .iter()
            .any(|r| r.original.stack_item_kind == crate::rl::StackItemKindV2::TriggeredAbility));
        assert_eq!(f.compatibility_terminals.len(), 1);
        assert!(matches!(
            f.compatibility_terminals[0].target,
            crate::time_stop_public_v68::CompatibilityTargetV68::DrakeCombat { occurrence: 0 }
        ));
        assert_eq!(
            f.compatibility_terminals[0].outcome,
            crate::time_stop_public_v68::TerminalOutcomeV68::Disposed
        );
        assert_eq!(f, facts(&restore(&s), p));
    }
}
#[test]
fn independent_initial_hidden_worlds_paid_disposal_public_facts_are_identical_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut s = ready_hidden(p, hidden);
            cast(&mut s, p, "Think Twice", &[], 2);
            cast(&mut s, p, "Time Stop", &[], 6);
            end_turn(&mut s);
            s
        };
        let a = make("Forest");
        let b = make("Counterspell");
        assert_ne!(a.state_hash(), b.state_hash());
        for viewer in [p, p.opponent()] {
            assert_eq!(facts(&a, viewer), facts(&b, viewer));
        }
    }
}
#[test]
fn independent_initial_private_hand_does_not_select_archived_public_scope_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut s = ready(p);
            // Independent initial native worlds, before any paid history.
            crate::policy_observation_v6::tests::put(&mut s, p.opponent(), hidden, Zone::Hand);
            cast(&mut s, p, "Think Twice", &[], 2);
            cast(&mut s, p, "Time Stop", &[], 6);
            end_turn(&mut s);
            s
        };
        let a = make("Forest");
        let b = make("Imprisoned in the Moon");
        assert_ne!(a.state_hash(), b.state_hash());
        for viewer in [p, p.opponent()] {
            let before = serde_json::to_vec(&b).unwrap();
            let actual = crate::time_stop_public_v68::capture(&b, viewer).unwrap();
            assert_eq!(
                actual,
                crate::time_stop_public_v68::capture(&a, viewer).unwrap()
            );
            assert_eq!(
                actual,
                crate::time_stop_public_v68::capture(&restore(&b), viewer).unwrap()
            );
            assert_eq!(serde_json::to_vec(&b).unwrap(), before);
        }
        // Current actor-visible admission remains strict. This test only
        // isolates archived public facts, not admission of the hidden printing.
        assert!(require_supported(&b, Some(p.opponent())).is_err());
        assert!(require_supported(&b, Some(p)).is_ok());
    }
}
