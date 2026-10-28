use super::*;
use crate::{
    effect::genesis_wave_v1::tests::{choose, finish, paid, ready, restore, subset},
    genesis_wave_public_v69::{self as public, StageV69},
    ids::PlayerId,
    state::Zone,
};
#[test]
fn genuine_paid_wave_public_reveal_subset_and_terminal_roundtrip_are_complete_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            p,
            &[
                "Forest",
                "Giant Growth",
                "Llanowar Elves",
                "Witness Protection",
            ],
            12,
            "Island",
        );
        crate::policy_observation_v6::tests::put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
        let wave = paid(&mut s, p, "Genesis Wave", Some(4), 7);
        let legal = subset(&mut s, p);
        for viewer in [p, p.opponent()] {
            let f = public::capture(&s, viewer).unwrap();
            assert_eq!(f.waves.len(), 1);
            assert_eq!(f.waves[0].revealed.len(), 4);
            assert_eq!(f.waves[0].stage, StageV69::Subset);
            assert_eq!(f.waves[0].x_value, 4);
            assert_eq!(
                f.waves[0].revealed.iter().filter(|r| r.eligible).count(),
                legal.len()
            );
            let text = serde_json::to_string(&f).unwrap();
            for forbidden in [
                "original_library",
                "remaining_library",
                "history_index",
                "GenesisWaveProgress",
                "original_sha256",
            ] {
                assert!(!text.contains(forbidden));
            }
            assert_eq!(f, public::capture(&restore(&s), viewer).unwrap());
            let mut v = serde_json::to_value(&f).unwrap();
            v["waves"][0]["original_library"] = serde_json::json!([]);
            assert!(serde_json::from_value::<public::FactsV69>(v).is_err());
        }
        choose(&mut s, &[]);
        finish(&mut s, wave);
        for viewer in [p, p.opponent()] {
            let f = public::capture(&s, viewer).unwrap();
            let w = &f.waves[0];
            assert_eq!(w.stage, StageV69::Finished);
            assert_eq!(w.revealed.len(), 4);
            assert!(w.selected_indices.is_empty());
            assert_eq!(w.remainder_indices.len(), 4);
            assert_eq!(w.spell_departure, Some(Zone::Graveyard));
            assert_eq!(f, public::capture(&restore(&s), viewer).unwrap());
        }
    }
}
#[test]
fn independent_prepaid_wave_public_hidden_libraries_and_orphan_progress_refuse_readonly_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let mut s = ready(p, &["Forest", "Giant Growth", "Llanowar Elves"], 8, hidden);
            paid(&mut s, p, "Genesis Wave", Some(3), 6);
            subset(&mut s, p);
            s
        };
        let a = make("Island");
        let b = make("Mountain");
        assert_ne!(a, b);
        for viewer in [p, p.opponent()] {
            assert_eq!(
                public::capture(&a, viewer).unwrap(),
                public::capture(&b, viewer).unwrap()
            );
        }
        let mut bad = a.clone();
        let mut e = bad
            .engine
            .event_history
            .iter()
            .find(|e| {
                matches!(
                    e,
                    crate::event::CommittedEvent::GenesisWaveProgressV1 { .. }
                )
            })
            .unwrap()
            .clone();
        if let crate::event::CommittedEvent::GenesisWaveProgressV1 { item, .. } = &mut e {
            item.controller = item.controller.opponent();
        }
        bad.engine.event_history.push(e);
        let bytes = serde_json::to_vec(&bad).unwrap();
        for viewer in [p, p.opponent()] {
            assert!(public::capture(&bad, viewer).is_err());
        }
        assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
        assert!(crate::end_turn_observation_v68::validate_state(&a).is_err());
        // A genuine finalized public cast later returned to a hidden library
        // cannot be relabelled by an independent private-slot permutation.
        // The cast/move are native; only that final permutation is a labelled
        // hidden-world boundary primitive, never a fabricated cast receipt.
        for (name, expected) in [
            ("Alesha, Who Laughs at Fate", true),
            ("Fiend Artisan", false),
        ] {
            let mut world = crate::call_observation_v62::tests::ready(p, "Forest");
            let card = crate::graveyard_return_public_v61::tests::paid(&mut world, p, name, None);
            crate::event::propose_and_commit(
                &mut world,
                crate::event::ProposedEvent::zone_change(card, Zone::Library),
            );
            assert!(crate::trigger::collect_and_process(&mut world).is_empty());
            assert_eq!(require_supported(&world, None).is_ok(), expected);
            world.objects.get_mut(card).card_def =
                crate::card_def::card_id_by_name(if expected { "Fiend Artisan" } else { "Forest" })
                    .unwrap();
            let before = world.clone();
            assert_eq!(require_supported(&world, None).is_ok(), expected);
            assert_eq!(world, before);
            let restored = restore(&world);
            assert_eq!(require_supported(&restored, None).is_ok(), expected);
        }
    }
}

#[test]
fn genuine_paid_wave_native_virtual_copy_public_lineage_and_terminal_are_exact_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(
            p,
            &[
                "Forest",
                "Giant Growth",
                "Llanowar Elves",
                "Witness Protection",
            ],
            12,
            "Island",
        );
        let original = paid(&mut s, p, "Genesis Wave", Some(4), 7);
        // Labelled genuine native virtual-copy primitive on a genuinely paid
        // original. This is not a claimed paid spell-copy printing.
        let (copy, _) = crate::engine::copy_spell_for(&mut s, original, p).unwrap();
        let original_row = s
            .stack
            .iter()
            .find(|r| r.source == original)
            .unwrap()
            .clone();
        let copy_row = s.stack.iter().find(|r| r.source == copy).unwrap().clone();
        assert!(copy_row.is_copy);
        assert!(copy_row
            .v4
            .source_contract
            .unwrap()
            .spell_copy_origin
            .is_some());
        subset(&mut s, p);
        choose(&mut s, &[]);
        for _ in 0..24 {
            let d = crate::effect::genesis_wave_v1::tests::next(&mut s);
            if !s
                .stack
                .iter()
                .any(|r| r.v4.stack_item_id == copy_row.v4.stack_item_id)
                && s.engine
                    .pending_effect
                    .as_ref()
                    .is_none_or(|r| r.resolving_item.v4.stack_item_id != copy_row.v4.stack_item_id)
            {
                break;
            }
            match d {
                crate::engine::Decision::ChooseEffectTargets { legal_targets, .. } => {
                    if let Some(target) = legal_targets.first() {
                        crate::engine::step(
                            &mut s,
                            crate::engine::Action::ChooseEffectTarget(*target),
                        )
                        .unwrap();
                    } else {
                        crate::engine::step(&mut s, crate::engine::Action::FinishEffectSelection)
                            .unwrap();
                    }
                }
                d => crate::effect::genesis_wave_v1::tests::pass(&mut s, d),
            }
        }
        for viewer in [p, p.opponent()] {
            let f = public::capture(&s, viewer).unwrap();
            let c = f
                .waves
                .iter()
                .find(|r| r.source.arena_id == copy.0)
                .unwrap();
            assert_eq!(c.stage, StageV69::Finished);
            assert_eq!(c.x_value, original_row.v4.x_value);
            assert!(c.is_copy);
            assert_eq!(c.spell_departure, None);
            assert_eq!(
                c.source.owner,
                copy_row.v4.source_contract.unwrap().owner.into()
            );
            assert_eq!(c.controller, copy_row.controller.into());
            assert_eq!(f, public::capture(&restore(&s), viewer).unwrap());
        }
        assert!(s.stack.iter().any(|r| r == &original_row));
    }
}
