use super::*;
#[test]
fn exsanguinate_native_mixed_pool_quote_and_wide_pending_selection_both_seats() {
    // Declared native mana-pool operands, not a claim of a gameplay mana loop.
    // The consumer case separately uses real lands and payment beyond255.
    let id = crate::card_def::card_id_by_name("Exsanguinate").unwrap();
    let maximum = u16::from(u8::MAX) * 6 - 2;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        state.players[actor.index()].mana_pool = [u8::MAX; 6];
        let spell =
            crate::policy_observation_v6::tests::put(&mut state, actor, "Exsanguinate", Zone::Hand);
        step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(
            matches!(advance_until_decision(&mut state), Decision::ChooseEffectOption { player, source, option_count } if player==actor && source==spell && option_count==maximum+1)
        );
        assert_eq!(
            maximum_payable_x(
                &card_def::CARD_DEFS[usize::from(id)],
                state.engine.pending_cast.as_ref().unwrap(),
                &state
            ),
            Some(maximum)
        );
        step(&mut state, Action::ChooseEffectOption(maximum)).unwrap();
        let pending = state.engine.pending_cast.as_ref().unwrap();
        assert_eq!(pending.selected_x_v59(), Some(maximum));
        assert_eq!(pending.x_value, None);
        validate_pending_cast(&state, pending).unwrap();
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        validate_pending_cast(&restored, restored.engine.pending_cast.as_ref().unwrap()).unwrap();
    }
}

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
