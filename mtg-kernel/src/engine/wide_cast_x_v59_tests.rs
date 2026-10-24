use super::*;
#[test]
fn u16_x_boundary_primitive_keeps_65535_and_65536_choice_cardinality_without_casting_stress() {
    // Arithmetic boundary operands, not a claim of a paid65535-mana cast.
    for (budget, expected) in [
        (2_u32, 0_u16),
        (257, 255),
        (258, 256),
        (65536, 65534),
        (65537, 65535),
    ] {
        assert_eq!(
            maximum_monotone_x_v59(u16::MAX, |x| u32::from(x) + 2 <= budget),
            Some(expected)
        );
    }
    assert_eq!(maximum_monotone_x_v59(u16::MAX, |_| false), None);
    assert_eq!(
        maximum_monotone_x_v59(255, |x| u32::from(x) + 2 <= 300),
        Some(255)
    );
    // A genuine older root keeps its frozen255 menu even when more
    // mana is available. A forged wide field cannot widen that owner.
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        for _ in 0..260 {
            crate::policy_observation_v6::tests::put(
                &mut state,
                actor,
                "Forest",
                Zone::Battlefield,
            );
        }
        let spell = crate::policy_observation_v6::tests::put(
            &mut state,
            actor,
            "Wildwood Scourge",
            Zone::Hand,
        );
        step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::ChooseEffectOption {
                option_count: 256,
                ..
            }
        ));
        step(&mut state, Action::ChooseEffectOption(255)).unwrap();
        assert_eq!(
            state.engine.pending_cast.as_ref().unwrap().x_value,
            Some(255)
        );
        state.engine.pending_cast.as_mut().unwrap().x_value = None;
        state.engine.pending_cast.as_mut().unwrap().wide_x_value_v59 = Some(256);
        let before = state.clone();
        assert!(
            validate_pending_cast(&state, state.engine.pending_cast.as_ref().unwrap()).is_err()
        );
        assert_eq!(state, before);
    }
    let count = u32::from(u16::MAX) + 1;
    assert_eq!(count, 65536);
    assert!(u16::try_from(count).is_err());
    let action = Action::ChooseEffectOption(u16::MAX);
    assert!(matches!(action, Action::ChooseEffectOption(65535)));
}
