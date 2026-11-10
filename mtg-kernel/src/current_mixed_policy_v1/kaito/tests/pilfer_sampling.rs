use super::*;
use crate::limited_match_v1::LimitedBeliefV1;

fn card(name: &str) -> u16 {
    crate::card_def::card_id_by_name(name).unwrap()
}

fn decision(owner: &KaitoSession) -> KaitoDecision {
    let FastActorResponseV1::Decision(expected) = owner.current_response() else {
        panic!("no owned Pilfer choice");
    };
    owner.encode(expected).unwrap()
}

fn slots(state: &GameState, objects: &[ObjectId]) -> Vec<u16> {
    objects
        .iter()
        .map(|&id| state.objects.get(id).card_def)
        .collect()
}

#[test]
fn full_pool_paid_pilfer_samples_revealed_names_and_rebinds_each_native_answer_both_seats() {
    let shown = [
        "Forest",
        "Giant Growth",
        "Hero's Downfall",
        "Hero's Downfall",
        "Plains",
    ];
    let paired = [
        "Hero's Downfall",
        "Forest",
        "Hero's Downfall",
        "Plains",
        "Giant Growth",
    ];
    let mut corpus = shown.map(card).to_vec();
    corpus.extend([card("Island"); 3]);
    corpus.extend([card("Forest"); 39]);
    corpus.extend([card("Swamp"); 2]);
    let belief = LimitedBeliefV1::new([corpus]);

    for p in [PlayerId::P0, PlayerId::P1] {
        let opponent = p.opponent();
        let hidden = |owner: &KaitoSession| {
            let state = owner.fixture_state();
            let other = &state.players[opponent.index()];
            (slots(state, &other.hand), slots(state, &other.library))
        };
        let paid = |names: [&str; 5], unknown: &str| {
            let (mut state, source) = scene(p);
            for name in names {
                put(&mut state, opponent, name, Zone::Hand);
            }
            put(&mut state, opponent, unknown, Zone::Library);
            full_pool_pending(p, state, source).0
        };
        let live = paid(shown, "Island");
        let other = paid(paired, "Mountain");
        let live_hidden = hidden(&live);
        let other_hidden = hidden(&other);
        assert_ne!(live_hidden.0, other_hidden.0);
        assert_ne!(live_hidden.1, other_hidden.1);
        let root = decision(&live);
        let other_root = decision(&other);
        assert_eq!(root.packet(), other_root.packet());

        // Ineligible revealed cards must be accounted for too. An unchanged
        // nonland menu alone cannot authorize a guessed complete hand.
        let mut forged_names = shown.map(card);
        forged_names[0] = card("Mountain");
        forged_names.sort_unstable();
        assert!(
            crate::effect::revealed_discard_sampling_v1::PreparedV1::capture(
                live.fixture_state(),
                p,
                &forged_names,
            )
            .is_err()
        );

        for seed in [552_653, 552_654] {
            let mut sampled = live
                .sampled_full_pool_closed_list_v1(&root, seed, &belief)
                .unwrap();
            let paired = other
                .sampled_full_pool_closed_list_v1(&other_root, seed, &belief)
                .unwrap();
            assert_eq!(hidden(&sampled), hidden(&paired));
            assert_ne!(hidden(&sampled).1, live_hidden.1);
            assert_eq!(
                sampled.inner.reveal_hand_minima_v3(),
                live.inner.reveal_hand_minima_v3(),
            );
            let d = decision(&sampled);
            assert_eq!(d.packet(), root.packet());
            assert_eq!(decision(&paired).packet(), root.packet());
            assert!(sampled.apply(&root, 0).is_err());
            if seed == 552_653 {
                assert_discard_family_guards(&root, &d, p);
            }

            let saved = sampled.snapshot();
            for public in 0..d.packet().actions().len() {
                sampled.restore(&saved).unwrap();
                assert!(sampled.apply(&d, public as u32).is_err());
                let fresh = decision(&sampled);
                assert_eq!(fresh.packet(), root.packet());
                let ordinal = fresh.current.name_order.as_ref().unwrap()[public];
                let A::ChooseEffectTarget {
                    target: TargetRefV1::Object { object },
                    ..
                } = &fresh.backend.actions()[ordinal as usize]
                else {
                    panic!("missing sampled discard object");
                };
                let discarded = ObjectId(object.arena_id);
                let native_index = fresh.backend.fixture_original_index_v75(ordinal).unwrap();
                let h = fresh.backend.expected();
                let mut native = sampled.inner.fixture_native_clone_v75();
                assert_eq!(
                    sampled.apply(&fresh, public as u32).unwrap(),
                    native.step(h.episode_id, h.step, native_index).unwrap(),
                );
                settle(&mut sampled, &mut native);
                let state = sampled.fixture_state();
                assert_eq!(state, native.kaito_fixture_state());
                assert_eq!(state.objects.get(discarded).zone, Zone::Graveyard);
                assert_eq!(state.objects.get(discarded).card_def, object.card_db_id);
                assert_eq!(state.players[opponent.index()].hand.len(), 4);
            }
        }
        assert_eq!(hidden(&live), live_hidden);
        assert_eq!(hidden(&other), other_hidden);
    }
}

fn assert_discard_family_guards(before: &KaitoDecision, after: &KaitoDecision, actor: PlayerId) {
    use crate::akroma_family_public_v40::FamilyProjectionV40;

    let after_families = after.backend.family_projections_v75();
    let matches = |families: Vec<FamilyProjectionV40>| {
        FamilyProjectionV40::sampled_revealed_discard_families_match(
            before.backend.family_projections_v75().to_vec(),
            families,
            &before.backend.candidates_v75().candidates,
            &after.backend.candidates_v75().candidates,
            actor.into(),
        )
    };
    assert!(matches(after_families.to_vec()));

    let mut changed = after_families.to_vec();
    let cerberus = changed.iter_mut().find_map(|family| match family {
        FamilyProjectionV40::Cerberus(observation) => Some(observation),
        _ => None,
    }).expect("the sampled discard still binds public creature-death facts");
    cerberus.creature_died_this_turn = !cerberus.creature_died_this_turn;
    assert!(!matches(changed));

    let mut changed = after_families.to_vec();
    let fishing = changed.iter_mut().find_map(|family| match family {
        FamilyProjectionV40::Fishing(observation) => Some(observation),
        _ => None,
    }).expect("the sampled discard still binds Fishing's classified actions");
    let A::ChooseEffectTarget { min_targets, .. } = &mut fishing.actions[0].semantic else {
        panic!("missing classified discard answer");
    };
    *min_targets += 1;
    assert!(!matches(changed));
}
