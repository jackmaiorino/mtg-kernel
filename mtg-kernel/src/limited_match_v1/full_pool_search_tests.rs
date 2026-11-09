//! Coverage of the executable pool at opening roots and actual paid Partial
//! spell roots. This is bounded search-contract evidence, not win-rate evidence.
use super::*;
use crate::effect::thousand_year_storm_v1_tests as paid;

fn names() -> Vec<String> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../data/limited/fdn_v1/booster_pool_v1.json"
    ))
    .unwrap();
    let names = manifest["card_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 286);
    names
}

fn hidden_pair(session: &FastActorSessionV1, actor: PlayerId) -> FastActorSessionV1 {
    let island = card_id_by_name("Island").unwrap();
    let unknown = unknown_slots_v1(session.game_state(), actor, actor.opponent(), &[]);
    assert!(!unknown.is_empty());
    session.census_edited_clone_v1(|state| {
        for id in unknown {
            let card = state.objects.get_mut(id);
            card.card_def = island;
            card.name = CARD_DEFS[island as usize].name.into();
            card.v4 = ObjectStateV4::from_card_def(island);
        }
    })
}

fn compare_worlds(
    session: &FastActorSessionV1,
    actor: PlayerId,
    belief: &LimitedBeliefV1,
    seed: u64,
) {
    let variant = hidden_pair(session, actor);
    let before = session.game_state().clone();
    let mode = WorldModeV1::ClosedListV3;
    let a = sample_world_v1(session, mode, Some(belief), seed).unwrap();
    let b = sample_world_v1(&variant, mode, Some(belief), seed).unwrap();
    assert_eq!(a.game_state(), b.game_state());
    assert_eq!(
        a.diagnostic_current_action_semantics().unwrap(),
        session.diagnostic_current_action_semantics().unwrap()
    );
    assert_eq!(
        b.diagnostic_current_action_semantics().unwrap(),
        a.diagnostic_current_action_semantics().unwrap()
    );
    let left = pimc_choose_v1(session, 2, 1, mode, Some(belief), seed).unwrap();
    let right = pimc_choose_v1(&variant, 2, 1, mode, Some(belief), seed).unwrap();
    assert_eq!(left, right);
    assert_eq!(left.1.visits.iter().sum::<u32>(), 2);
    assert_eq!(left.1.rejected, 0);
    assert_eq!(left.1.rollout_errors, 0);
    assert_eq!(session.game_state(), &before);
}

#[test]
fn closed_list_v3_frozen_286_opening_roots_ignore_hidden_composition_both_seats() {
    let forest = card_id_by_name("Forest").unwrap();
    let mut ids = HashSet::new();
    for name in names() {
        let id = card_id_by_name(&name).unwrap();
        assert!(ids.insert(id), "duplicate frozen printing {name}");
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut decks = [vec![forest; 40], vec![forest; 40]];
            decks[actor.index()].fill(id);
            let mut corpus = vec![forest; 40];
            corpus[..4].fill(id);
            let belief = LimitedBeliefV1::new([corpus]);
            let mut session = rules_only_closed_list_game(decks, actor);
            session.enable_limited_reveals_v3().unwrap();
            assert!(session.game_state().players[actor.index()]
                .hand
                .iter()
                .all(|&object| session.game_state().objects.get(object).card_def == id));
            assert!(!session.game_state().players[actor.index()].hand.is_empty());
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("{name}: no opening decision")
            };
            assert_eq!(seat(decision.acting_player), actor);
            compare_worlds(&session, actor, &belief, 552_286 + u64::from(id));
        }
    }
}

#[test]
fn closed_list_v3_rules_only_leyline_opponent_uses_sampled_kept_hand() {
    use crate::rl::ActionSemanticV1;
    let axe = card_id_by_name("Leyline Axe").unwrap();
    let forest = card_id_by_name("Forest").unwrap();
    let belief = LimitedBeliefV1::new([vec![axe; 40]]);
    let apply = |session: &mut FastActorSessionV1, select: fn(&ActionSemanticV1) -> bool| {
        let menu = session.diagnostic_current_action_semantics().unwrap();
        let index = menu.iter().position(select).expect("actual opening choice");
        step_index(session, index as u32, false).unwrap();
    };
    for starting in [PlayerId::P0, PlayerId::P1] {
        let opponent = starting.opponent();
        let mut decks = [vec![forest; 40], vec![forest; 40]];
        decks[starting.index()].fill(axe);
        let mut live = rules_only_closed_list_game(decks, starting);
        live.enable_limited_reveals_v3().unwrap();
        for _ in 0..2 {
            apply(&mut live, |action| {
                matches!(
                    action,
                    ActionSemanticV1::ChooseLondonMulligan {
                        mulligan: false,
                        ..
                    }
                )
            });
        }
        let before = live.game_state().clone();
        assert!(before.players[opponent.index()]
            .hand
            .iter()
            .all(|&object| before.objects.get(object).card_def == forest));
        let mut sampled =
            sample_world_v1(&live, WorldModeV1::ClosedListV3, Some(&belief), 552_482).unwrap();
        assert_eq!(
            sampled.diagnostic_current_action_semantics().unwrap(),
            live.diagnostic_current_action_semantics().unwrap()
        );
        for _ in 0..before.players[starting.index()].hand.len() {
            apply(&mut sampled, |action| {
                matches!(
                    action,
                    ActionSemanticV1::ChooseEffectOption {
                        option_index: 0,
                        ..
                    }
                )
            });
        }
        let FastActorResponseV1::Decision(current) = sampled.current_response() else {
            panic!("sampled future Leyline chooser did not surface")
        };
        assert_eq!(seat(current.acting_player), opponent);
        apply(&mut sampled, |action| {
            matches!(
                action,
                ActionSemanticV1::ChooseEffectOption {
                    option_index: 1,
                    ..
                }
            )
        });
        assert!(sampled.game_state().players[opponent.index()]
            .battlefield
            .iter()
            .any(|&object| sampled.game_state().objects.get(object).card_def == axe));
        assert_eq!(live.game_state(), &before);
    }
}

#[test]
fn closed_list_v3_paid_partial_roots_ignore_hidden_composition_both_seats() {
    let forest = card_id_by_name("Forest").unwrap();
    let partial = names()
        .into_iter()
        .filter(|name| {
            CARD_DEFS[card_id_by_name(name).unwrap() as usize].capability
                == crate::card_def::CardCapability::Partial
        })
        .collect::<Vec<_>>();
    assert_eq!(partial.len(), 36);
    for name in partial {
        let printing = card_id_by_name(&name).unwrap();
        for actor in [PlayerId::P0, PlayerId::P1] {
            // Native initial mana/long-library setup; every spell and the
            // shared target creature below is then cast with real land taps.
            let mut state = paid::ready(actor, "Forest");
            state.reveal_meaning_v3.enable();
            let creature = paid::cast(&mut state, actor, "Llanowar Elves", &[], 1);
            paid::finish(&mut state);
            let targets = if matches!(name.as_str(), "Electroduplicate" | "Imprisoned in the Moon")
            {
                vec![crate::state::Target::Object(creature)]
            } else {
                vec![]
            };
            let definition = &CARD_DEFS[printing as usize];
            if definition.is_land {
                assert_eq!(name, "Secluded Courtyard");
                let land =
                    crate::policy_observation_v6::tests::put(&mut state, actor, &name, Zone::Hand);
                paid::action(&mut state, crate::engine::Action::PlayLand(land));
            } else {
                let cost = usize::from(definition.cost.generic) + definition.cost.pips.len();
                let spell = paid::cast(&mut state, actor, &name, &targets, cost);
                assert!(state
                    .stack
                    .iter()
                    .any(|item| item.source == spell && !item.is_copy));
                // A real playable instant gives these priority windows a
                // second action, so this checks actual PIMC visits as well
                // as explicit world sampling.
                crate::policy_observation_v6::tests::put(
                    &mut state,
                    actor,
                    "Lightning Bolt",
                    Zone::Hand,
                );
            }
            let mut corpus = state
                .objects
                .iter()
                .filter(|(_, card)| card.owner == actor.opponent() && !card.v4.is_token)
                .map(|(_, card)| card.card_def)
                .collect::<Vec<_>>();
            corpus.sort_unstable();
            let belief = LimitedBeliefV1::new([corpus]);
            let mut session =
                rules_only_closed_list_game([vec![forest; 40], vec![forest; 40]], actor);
            session.install_rules_only_test_state_v1(state);
            let FastActorResponseV1::Decision(decision) = session.current_response() else {
                panic!("{name}: paid root did not surface")
            };
            assert_eq!(seat(decision.acting_player), actor, "{name}: root actor");
            assert!(
                decision.legal_action_count > 1,
                "{name}: search needs a real alternative"
            );
            compare_worlds(&session, actor, &belief, 552_360 + u64::from(printing));
        }
    }
}
