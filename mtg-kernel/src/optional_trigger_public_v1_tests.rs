//! Source fixtures only. Primitive worlds are visibility inputs, not gameplay producers.
use crate::{
    card_def::card_id_by_name,
    ids::PlayerId,
    policy_observation_v6::tests::{put, ready_state_for},
    state::{FaceDownV1, Zone},
};

#[test]
fn exact_printed_meanings_keep_order_and_reject_rehashed_semantic_forgery() {
    let spinner = card_id_by_name("Spinner of Souls").unwrap();
    let quandary = card_id_by_name("Painful Quandary").unwrap();
    assert_eq!((spinner, quandary), (446, 447));
    let mut s = crate::rules_meaning_spinner_v54::printed_meaning(spinner).unwrap();
    assert_eq!(s.printed.generic_mana, 2);
    assert_eq!(s.printed.colored_pips, vec![crate::mana::ManaColor::G]);
    assert_eq!(s.ordered_rules.len(), 2);
    assert!(!s.model_admitted);
    s.ordered_rules.reverse();
    s.root_sha256 = crate::optional_trigger_source_v1::digest(&s.ordered_rules).unwrap();
    assert!(crate::rules_meaning_spinner_v54::validate(&s).is_err());
    let mut q = crate::rules_meaning_quandary_v55::printed_meaning(quandary).unwrap();
    assert_eq!(q.printed.generic_mana, 3);
    assert_eq!(q.printed.colored_pips, vec![crate::mana::ManaColor::B; 2]);
    let crate::rules_meaning_quandary_v55::InstructionV55::OpponentGenuinelyCastsSpell {
        ordered_resolution,
        ..
    } = &mut q.ordered_rules[0];
    ordered_resolution.reverse();
    q.root_sha256 = crate::optional_trigger_source_v1::digest(&q.ordered_rules).unwrap();
    assert!(crate::rules_meaning_quandary_v55::validate(&q).is_err());
    for name in [
        "Drake Hatcher",
        "Flamewake Phoenix",
        "Elenda, Saint of Dusk",
    ] {
        let id = card_id_by_name(name).unwrap();
        assert!(crate::rules_meaning_spinner_v54::printed_meaning(id).is_err());
        assert!(crate::rules_meaning_quandary_v55::printed_meaning(id).is_err());
    }
}

#[test]
fn independent_hidden_and_face_down_worlds_have_equal_public_optional_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut worlds = Vec::new();
        for concealed in ["Spinner of Souls", "Llanowar Elves"] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor.opponent(), concealed, Zone::Hand);
            put(
                &mut state,
                actor.opponent(),
                if concealed == "Spinner of Souls" {
                    "Painful Quandary"
                } else {
                    "Forest"
                },
                Zone::Library,
            );
            let face = put(&mut state, actor.opponent(), concealed, Zone::Battlefield);
            state.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            // Both effective public bases are 2/2. Underlying 4/3 versus 1/1 must not
            // select either a printing row or a different arithmetic refusal.
            state.objects.get_mut(face).counters.plus1_plus1 = i32::MAX - 2;
            worlds.push((
                crate::spinner_public_v54::capture(&state).unwrap(),
                crate::quandary_public_v55::capture(&state, actor).unwrap(),
            ));
        }
        assert_eq!(worlds[0], worlds[1]);
    }
}

#[test]
fn public_selected_counters_are_full_width_and_dto_tampering_is_not_authority() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let id = put(&mut state, actor, "Spinner of Souls", Zone::Battlefield);
        let card = state.objects.get_mut(id);
        card.counters.charge = i32::MAX;
        card.counters.soul = i32::MAX;
        card.counters.incubation = i32::MAX;
        card.damage = u32::MAX;
        let facts = crate::spinner_public_v54::capture(&state).unwrap();
        let current = &facts.current[0];
        assert_eq!(
            (
                current.counters.charge,
                current.counters.soul,
                current.counters.incubation
            ),
            (i32::MAX, i32::MAX, i32::MAX)
        );
        assert_eq!(current.marked_damage, u32::MAX);
        let restored = serde_json::from_slice(&serde_json::to_vec(&facts).unwrap()).unwrap();
        crate::spinner_public_v54::validate(&state, &restored).unwrap();
        let mut forged = facts.clone();
        forged.current[0].counters.soul = 0;
        assert!(crate::spinner_public_v54::validate(&state, &forged).is_err());
    }
}
