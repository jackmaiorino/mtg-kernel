//! Source fixtures. Genuine paid native roots; all Rust remains unrun.
use super::*;
use crate::{liliana_public_v73 as facts, rl::PlayerSeatV1};
fn capture(s: &GameState, v: PlayerId) -> facts::FactsV73 {
    let before = s.clone();
    let f = facts::capture(s, v).unwrap();
    assert_eq!(*s, before);
    facts::validate_current(s, v, &f).unwrap();
    let restored: facts::FactsV73 =
        serde_json::from_slice(&serde_json::to_vec(&f).unwrap()).unwrap();
    assert_eq!(restored, f);
    assert!(!f.complete_ordinary_observation && !f.owning_consumer_admitted);
    f
}
#[test]
fn paid_plus_one_original_payment_token_replacement_and_wide_current_fields_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let mut s = ready(p, "Island");
            if season {
                pay(&mut s, p, "Doubling Season", &[], 5);
                finish(&mut s);
            }
            let v = walker(&mut s, p);
            // Explicit public counter-storage operands, not a claim of paid effects.
            s.objects.get_mut(v).counters.stash = 70_000;
            s.objects.get_mut(v).counters.fellowship = 80_000;
            let original = s.objects.get(v).zone_change_count;
            activate(&mut s, p, v, 0);
            for viewer in [p, p.opponent()] {
                let f = capture(&s, viewer);
                let a = &f.activations[0];
                assert_eq!(a.stage, facts::ActivationStageV73::Published);
                assert_eq!(a.ability_index, 0);
                assert_eq!(
                    (a.loyalty_before, a.loyalty_after),
                    if season { (12, 13) } else { (6, 7) }
                );
                let current = f.battlefield[p.index()]
                    .iter()
                    .find(|o| o.card.object == v.0)
                    .unwrap();
                assert_eq!(
                    (current.counters.stash, current.counters.fellowship),
                    (70_000, 80_000)
                );
                assert!(current.form.types.contains(&CardType::Planeswalker));
                assert_eq!(f.meaning.printed.loyalty_costs, [1, -4, -9]);
                assert_eq!(f.meaning.printed.zombie_card_id, 492);
                assert!(!f.meaning.full_engine_admitted && !f.meaning.complete_consumer_admitted);
            }
            // Native concealment primitive after a genuine paid publication.
            // It changes the current form, never the originally public source.
            let mut concealed = restore(&s);
            concealed.objects.get_mut(v).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            for viewer in [p, p.opponent()] {
                let f = capture(&concealed, viewer);
                let source = &f.activations[0].source;
                assert_eq!(
                    source.original_source.identity,
                    crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 491 }
                );
                assert!(matches!(
                    source.current_original_source.as_ref().unwrap().identity,
                    crate::optional_trigger_public_v1::IdentityV1::FaceDown { disguised: false }
                ));
            }
            event::propose_and_commit(&mut s, event::ProposedEvent::zone_change(v, Zone::Hand));
            event::propose_and_commit(
                &mut s,
                event::ProposedEvent::zone_change_to_battlefield_under_controller(v, p.opponent()),
            );
            let mut restored = restore(&s);
            finish(&mut s);
            finish(&mut restored);
            assert_eq!(s, restored);
            for viewer in [p, p.opponent()] {
                let f = capture(&s, viewer);
                let a = &f.activations[0];
                assert_eq!(a.source.original_source.generation, original);
                assert_eq!(a.source.captured_controller, PlayerSeatV1::from(p));
                assert!(a.source.current_original_source.is_none());
                assert_eq!(a.stage, facts::ActivationStageV73::NativeCompleted);
                assert_eq!(a.created_zombies.len(), if season { 2 } else { 1 });
                assert!(a
                    .created_zombies
                    .iter()
                    .all(|t| t.owner == PlayerSeatV1::from(p) && t.generation == 0));
            }
            let f = capture(&s, p);
            let mut forged = f.clone();
            forged.activations[0].loyalty_after += 1;
            assert!(facts::validate_current(&s, p, &forged).is_err());
            let mut raw = serde_json::to_value(&f).unwrap();
            raw["activations"][0]["source"]["current_original_source"] =
                serde_json::json!({"world":0});
            assert!(serde_json::from_value::<facts::FactsV73>(raw).is_err());
            assert!(crate::rl::observe_v1(&s, p, 0).is_err());
        }
    }
}
#[test]
fn paid_minus_four_original_apnap_scene_partial_prefix_batch_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let own = ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"]
            .map(|n| put(&mut s, p, n, Zone::Battlefield));
        let other = ["Llanowar Elves", "Healer's Hawk", "Savannah Lions"]
            .map(|n| put(&mut s, p.opponent(), n, Zone::Battlefield));
        let v = walker(&mut s, p);
        activate(&mut s, p, v, 1);
        prompt(&mut s);
        engine::step(&mut s, Action::ChooseEffectTarget(Target::Object(own[1]))).unwrap();
        let f = capture(&s, p);
        let r = &f.resolutions[0];
        let c = r.selections.iter().find(|c| c.current).unwrap();
        assert_eq!(r.active_player, PlayerSeatV1::from(p));
        assert!(!r.ultimate);
        assert_eq!(c.kind, facts::SelectionKindV73::SacrificeCreatures);
        assert_eq!(c.actor, PlayerSeatV1::from(p));
        assert_eq!(c.exact_count, 2);
        assert_eq!(
            c.selected_prefix
                .iter()
                .map(|r| r.object)
                .collect::<Vec<_>>(),
            vec![own[1].0]
        );
        assert!(!r.finished && r.completed_sacrifices.is_empty());
        assert!(own
            .into_iter()
            .chain(other)
            .all(|id| s.objects.get(id).zone == Zone::Battlefield));
        let mut restored = restore(&s);
        finish(&mut s);
        finish(&mut restored);
        assert_eq!(s, restored);
        let f = capture(&s, p.opponent());
        let r = &f.resolutions[0];
        assert!(r.finished);
        assert_eq!(r.completed_sacrifices.len(), 4);
        assert_eq!(r.chosen_sacrifices[p.index()].len(), 2);
        assert_eq!(r.chosen_sacrifices[p.opponent().index()].len(), 2);
        assert_eq!(f.deaths.len(), 2);
        assert!(f.deaths.iter().all(|d| d.collected));
        assert!(f.current_draws.is_empty());
        assert_eq!(
            f.activations[0].stage,
            facts::ActivationStageV73::NativeCompleted
        );
    }
}
#[test]
fn paid_ultimate_multitype_keeper_union_original_types_and_unknown_nested_field_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        pay(&mut s, p, "Doubling Season", &[], 5);
        finish(&mut s);
        let v = walker(&mut s, p);
        let hart = put(&mut s, p.opponent(), "Burnished Hart", Zone::Battlefield);
        let druid = put(&mut s, p.opponent(), "Paradise Druid", Zone::Battlefield);
        let pick = put(&mut s, p.opponent(), "Goldvein Pick", Zone::Battlefield);
        activate(&mut s, p, v, 2);
        // Land stage has many actual native setup lands; all five type stages
        // retain their fixed original roster, including empty represented types.
        let (_, _, legal, _) = prompt(&mut s);
        let Target::Object(land) = legal[0] else {
            panic!()
        };
        answer(&mut s, &[land]);
        answer(&mut s, &[hart]);
        prompt(&mut s);
        let f = capture(&s, p.opponent());
        let r = &f.resolutions[0];
        let c = r.selections.iter().find(|c| c.current).unwrap();
        assert_eq!(
            c.kind,
            facts::SelectionKindV73::KeepPermanent(CardType::Artifact)
        );
        assert!(c.candidates.iter().any(|b| b.object == hart.0));
        let original = r.rosters[p.opponent().index()]
            .iter()
            .find(|b| b.original.object == hart.0)
            .unwrap();
        assert!(
            original.permanent_types.contains(&CardType::Creature)
                && original.permanent_types.contains(&CardType::Artifact)
        );
        let mut restored = restore(&s);
        for branch in [&mut s, &mut restored] {
            answer(branch, &[hart]);
            finish(branch);
        }
        assert_eq!(s, restored);
        let f = capture(&s, p);
        let r = &f.resolutions[0];
        assert!(r.ultimate && r.finished);
        assert_eq!(
            r.keepers
                .iter()
                .filter(|k| k.permanent.object == hart.0)
                .count(),
            2
        );
        assert!(!r.completed_sacrifices.iter().any(|b| b.object == hart.0));
        assert!(r.completed_sacrifices.iter().any(|b| b.object == druid.0));
        assert!(r.completed_sacrifices.iter().any(|b| b.object == pick.0));
        let mut raw = serde_json::to_value(&f).unwrap();
        raw["resolutions"][0]["rosters"][p.opponent().index()][0]["history_index"] = 0.into();
        assert!(serde_json::from_value::<facts::FactsV73>(raw).is_err());
    }
}
#[test]
fn paid_observer_prebatch_token_self_sourcegone_and_private_visibility_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Island");
        let v = walker(&mut s, p);
        activate(&mut s, p, v, 0);
        finish(&mut s);
        let token = *s.players[p.index()]
            .battlefield
            .iter()
            .find(|o| token_definition(s.objects.get(**o).card_def))
            .unwrap();
        let elf = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
        let generation = s.objects.get(v).zone_change_count;
        crate::standard_cards_v1::restore_source_type_crew_input(&mut s, vec![(v, generation)]);
        event::propose_and_commit_batch(
            &mut s,
            [v, token, elf]
                .into_iter()
                .map(|o| event::ProposedEvent::zone_change(o, Zone::Graveyard))
                .collect(),
        );
        let f = capture(&s, p.opponent());
        assert_eq!(f.deaths.len(), 3);
        assert!(f
            .deaths
            .iter()
            .all(|d| d.observer.original_source.object == v.0
                && d.observer.original_source.generation == generation
                && d.observer.current_original_source.is_none()));
        assert!(f.deaths.iter().any(|d| d.creature.object == token.0));
        assert!(f.deaths.iter().any(|d| d.creature.object == v.0));
        let _ = next(&mut s);
        let published = capture(&s, p.opponent());
        assert_eq!(published.current_draws.len(), 3);
        assert!(published.deaths.iter().all(|d| d.collected));
        assert!(published
            .current_draws
            .iter()
            .all(|d| d.source.original_source.object == v.0
                && d.source.original_source.generation == generation
                && d.source.captured_controller == PlayerSeatV1::from(p)));
        let mut forged = published.clone();
        forged.deaths[0].observer.original_source.generation += 1;
        assert!(facts::validate_current(&s, p.opponent(), &forged).is_err());
        let mut restored = restore(&s);
        finish(&mut s);
        finish(&mut restored);
        assert_eq!(s, restored);
        assert!(capture(&s, p).deaths.iter().all(|d| d.collected));
        let hidden = put(&mut s, p, NAME, Zone::Hand);
        assert!(capture(&s, p)
            .authorized_printings
            .iter()
            .any(|r| r.object == hidden.0));
        assert!(!capture(&s, p.opponent())
            .authorized_printings
            .iter()
            .any(|r| r.object == hidden.0));
    }
}
