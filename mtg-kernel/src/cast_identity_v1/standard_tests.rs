//! The registered Standard printing supplies the real Disguise cast route.
use super::*;
use crate::{
    card_def,
    engine::{self, Action, CastMode, Decision},
    policy_observation_v6::tests::put,
    state::Step,
};

fn receipt_for(state: &GameState, spell: ObjectId) -> ReceiptV1 {
    state
        .engine
        .event_history
        .iter()
        .find_map(|event| match event {
            CommittedEvent::PublishedCastIdentityV1 { receipt } if receipt.spell == spell => {
                Some(*receipt)
            }
            _ => None,
        })
        .expect("genuine paid cast identity")
}

#[test]
fn genuinely_paid_concealed_cast_publishes_no_private_printing_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 40],
            &[forest; 40],
            crate::rl::card_name,
            110_632,
            actor,
        );
        state.step = Step::Main1;
        let lands: Vec<_> = (0..3)
            .map(|_| put(&mut state, actor, "Forest", Zone::Battlefield))
            .collect();
        let spell = put(&mut state, actor, "Flourishing Bloom-Kin", Zone::Hand);
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseCastMode { .. }
        ));
        engine::step(&mut state, Action::ChooseCastMode(CastMode::Alternative)).unwrap();
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == actor)
        );
        assert!(lands.iter().all(|land| state.objects.get(*land).tapped));
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        assert!(state.objects.get(spell).v4.face_down_v1.is_some());
        let receipt = receipt_for(&state, spell);
        assert_eq!(
            receipt.public_identity,
            PublicIdentityV1::Concealed { disguised: true }
        );
        let published = serde_json::to_string(&receipt).unwrap();
        assert!(!published.contains("Flourishing Bloom-Kin"));
        assert!(!published.contains("card_def"));
        assert!(!unsupported_public_history(&state, |_| true).unwrap());
        let restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(receipt_for(&restored, spell), receipt);
        // Direct private-field mutation checks this historical reader only.
        // It does not claim another legal Disguise cast or sampled full world.
        let mut other_hidden = restored;
        other_hidden.objects.get_mut(spell).card_def = forest;
        let before = other_hidden.clone();
        assert!(!unsupported_public_history(&other_hidden, |_| true).unwrap());
        assert_eq!(other_hidden, before);
    }
}
