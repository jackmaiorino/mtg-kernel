use super::*;
use crate::policy_observation_v6::tests::put;
use crate::rl_session::{FastActorResponseV1, FastActorSessionV1};
use crate::state::Zone;

#[test]
fn registered_predecessor_projection_private_fields_and_ordered_actions_are_exact_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = crate::extra_turn_v1::tests::ready(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let old = FastActorSessionV1::from_hunter_v45_fixture_state(state.clone());
        let new = FastActorSessionV1::from_eaten_v46_fixture_state(state);
        let FastActorResponseV1::Decision(e) = old.current_response() else {
            panic!("old ordinary decision missing")
        };
        let FastActorResponseV1::Decision(n) = new.current_response() else {
            panic!("new ordinary decision missing")
        };
        let (a, actions, _) = old
            .hunter_current_decision_input_v45(e, actor.into())
            .unwrap();
        let (b, new_actions, _) = new
            .eaten_current_decision_input_v46(n, actor.into())
            .unwrap();
        assert_eq!(actions, new_actions);
        assert_eq!(a.projection, b.projection);
        assert_eq!(a.extensions, b.extensions);
        assert_eq!(a.own_hand, b.own_hand);
        assert_eq!(a.known_hand_cards, b.known_hand_cards);
        assert_eq!(a.known_library_cards, b.known_library_cards);
        assert_eq!(a.known_face_down_cards, b.known_face_down_cards);
        assert_eq!(a.families, b.families);
        assert_eq!(a.divine, b.divine);
        assert_eq!(a.ajani, b.ajani);
        assert_eq!(a.faebloom, b.faebloom);
        assert_eq!(a.hunter, b.hunter);
        assert_eq!(a.trigger_order, b.trigger_order);
        assert!(b.eaten.pending.is_none() && b.eaten.payments.is_empty());
        assert_eq!(
            b.eaten.meanings,
            crate::eaten_public_v46::meanings().unwrap()
        );
        b.validate_artifact().unwrap();
        let mut bad = b;
        bad.eaten.producer_sha256.push_str("stale");
        bad.projection_sha256 = commitment(&bad).unwrap();
        assert!(bad.validate_artifact().is_err());
    }
}

#[test]
#[ignore = "requires serial Eaten Alive registration and native qualification"]
fn actual_owned_pending_cost_and_unknown_opponent_hand_projection_are_bound_both_seats() {
    use crate::engine::{Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::{GameState, Step, Target};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 20],
            &[forest; 20],
            |id| {
                crate::card_def::CARD_DEFS[usize::from(id)]
                    .object_name
                    .into()
            },
            110_462,
            actor,
        );
        state.step = Step::Main1;
        let spell = put(&mut state, actor, "Eaten Alive", Zone::Hand);
        for name in ["Savannah Lions", "Tolarian Terror"] {
            let id = put(&mut state, actor, name, Zone::Hand);
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(id, Zone::Battlefield),
            );
        }
        let recipient = put(&mut state, actor.opponent(), "Llanowar Elves", Zone::Hand);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(recipient, Zone::Battlefield),
        );
        let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        crate::engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(
            crate::engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        crate::engine::step(&mut state, Action::ChooseTarget(Target::Object(recipient))).unwrap();
        assert!(
            matches!(crate::engine::advance_until_decision(&mut state), Decision::ChooseEffectOption { source, option_count: 2, .. } if source == spell)
        );
        let before = serde_json::to_vec(&state).unwrap();
        let session = FastActorSessionV1::from_eaten_v46_fixture_state(state.clone());
        let FastActorResponseV1::Decision(current) = session.current_response() else {
            panic!("actual mandatory cost menu");
        };
        let (observation, actions, _) = session
            .eaten_current_decision_input_v46(current, actor.into())
            .unwrap();
        let pending = observation.eaten.pending.as_ref().unwrap();
        assert_eq!(pending.source.arena_id, spell.0);
        assert_eq!(
            pending.payable_branches,
            [
                crate::eaten_public_v46::BranchV46::SacrificeCreature,
                crate::eaten_public_v46::BranchV46::AdditionalMana3B
            ]
        );
        assert_eq!(pending.sacrifice_candidates.len(), 2);
        assert!(!pending.in_progress_payment);
        assert_eq!(actions.len(), 2);
        observation.validate_artifact().unwrap();
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert!(state.known_hand_cards(actor, actor.opponent()).is_empty());
        let mut substituted = state.clone();
        let island = crate::card_def::card_id_by_name("Island").unwrap();
        substituted.objects.get_mut(hidden).card_def = island;
        substituted.objects.get_mut(hidden).v4 = crate::state::ObjectStateV4::from_card_def(island);
        let session = FastActorSessionV1::from_eaten_v46_fixture_state(substituted);
        let FastActorResponseV1::Decision(current) = session.current_response() else {
            panic!("same actual menu");
        };
        let (changed, changed_actions, _) = session
            .eaten_current_decision_input_v46(current, actor.into())
            .unwrap();
        assert_eq!(observation, changed);
        assert_eq!(actions, changed_actions);
        let mut forged = observation;
        forged.eaten.meanings[0].additional_cost_mandatory = false;
        forged.projection_sha256 = commitment(&forged).unwrap();
        assert!(forged.validate_artifact().is_err());
    }
}
