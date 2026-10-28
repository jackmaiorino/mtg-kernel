//! mtg-kernel#380: an ability waiting on the stack whose source is back in a
//! hidden zone keeps that source's public definition in every sampled world.
use super::*;
use crate::rl_session::FastActorResponseV1;

fn all_decks() -> DeckPrior {
    DeckPrior::new(&(0..RUNTIME_DECKS.len()).collect::<Vec<_>>())
}

/// The opponent's unknown slots that a public source contract names.
fn pinned_slots(state: &GameState, actor: PlayerId) -> Vec<ObjectId> {
    let pinned = contract_pinned(state);
    unknown_slots(state, actor, actor.opponent())
        .into_iter()
        .filter(|id| pinned.contains(id))
        .collect()
}

/// Samples `seeds` worlds and checks that none fails to encode, each keeps
/// every pinned object's definition, and the prior still relabels the other
/// unknown slots.
fn assert_samples_keep_pinned(session: &FastActorSessionV1, actor: PlayerId, seeds: u64) {
    let prior = all_decks();
    let pinned = pinned_slots(session.game_state(), actor);
    assert!(!pinned.is_empty(), "fixture must exercise a pinned slot");
    let mut relabeled = false;
    for seed in 1..=seeds {
        let mut failure = None;
        let world = sample_recorded(session, seed, &prior, &mut failure)
            .unwrap_or_else(|e| panic!("seed {seed}: {e} {failure:?}"));
        assert!(failure.is_none(), "seed {seed}: {failure:?}");
        for &id in &pinned {
            let (root, sampled) = (
                session.game_state().objects.get(id),
                world.world.game_state().objects.get(id),
            );
            assert_eq!(
                sampled.card_def, root.card_def,
                "seed {seed}: object {id:?}"
            );
            assert_eq!(sampled.name, root.name);
            assert_eq!(sampled.zone_change_count, root.zone_change_count);
        }
        relabeled |= unknown_slots(session.game_state(), actor, actor.opponent())
            .iter()
            .any(|&id| {
                world.world.game_state().objects.get(id).card_def
                    != session.game_state().objects.get(id).card_def
            });
    }
    assert!(relabeled, "the prior must still relabel unpinned slots");
}

#[test]
fn prior_keeps_a_waiting_ability_source_shuffled_into_the_library() {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    let (actor, opp) = (PlayerId::P0, PlayerId::P1);
    let lembas_def = crate::card_def::card_id_by_name("Lembas").unwrap();
    let deck = RUNTIME_DECKS
        .iter()
        .position(|d| d.card_ids.contains(&lembas_def))
        .expect("a registered deck plays Lembas");
    let mut state = ready_state_for(opp);
    let mut rest = RUNTIME_DECKS[deck].card_ids.to_vec();
    rest.remove(rest.iter().position(|&c| c == lembas_def).unwrap());
    for &def in &rest {
        put(&mut state, opp, CARD_DEFS[def as usize].name, Zone::Library);
    }
    let lembas = put(&mut state, opp, "Lembas", Zone::Battlefield);
    for name in ["Forest", "Mountain", "Island", "Swamp", "Counterspell"] {
        put(&mut state, actor, name, Zone::Library);
    }
    put(&mut state, actor, "Lightning Bolt", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[opp.index()].mana_pool[ManaColor::C.pool_index()] = 2;
    state.active_player = opp;
    state.priority_player = opp;
    engine::step(&mut state, Action::ActivateAbility(lembas, 0)).unwrap();
    // Pass until Lembas's leave trigger has shuffled it into the library and
    // the actor holds priority over the waiting Food ability.
    let mut reached = false;
    for _ in 0..16 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(lembas).zone == Zone::Library && state.priority_player == actor {
            reached = true;
            break;
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    assert!(reached, "fixture did not reach the waiting Food ability");
    let item = state.stack.last().expect("Food ability waits");
    assert_eq!(item.source, lembas);
    let contract = item.v4.ability_source_contract.expect("frozen source");
    assert_eq!(contract.card_def, lembas_def);
    assert!(state.objects.get(lembas).zone_change_count > contract.zone_change_count);
    assert_eq!(pinned_slots(&state, actor), vec![lembas]);

    let session = FastActorSessionV1::from_v3_fixture_state(state);
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        panic!("fixture has no live decision")
    };
    assert_eq!(d.acting_player, crate::rl::PlayerSeatV1::from(actor));
    assert_samples_keep_pinned(&session, actor, 16);
}

#[test]
fn s1_lembas_roots_sample_without_boundary_encoding_failures() {
    for (fixture, legal) in [
        (include_str!("fixtures/lembas_root_o2_v0_f2_s0.json"), 2),
        (include_str!("fixtures/lembas_root_o2_v0_f3_s0.json"), 2),
        (include_str!("fixtures/lembas_root_o2_v0_f5_s1.json"), 3),
    ] {
        let record: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let state: GameState = serde_json::from_value(record["state"].clone()).unwrap();
        let actor = if record["actor"] == 0 {
            PlayerId::P0
        } else {
            PlayerId::P1
        };
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let FastActorResponseV1::Decision(d) = session.current_response() else {
            panic!("{}: no live decision", record["case"])
        };
        assert_eq!(
            d.acting_player,
            crate::rl::PlayerSeatV1::from(actor),
            "{}",
            record["case"]
        );
        assert_eq!(d.legal_action_count, legal, "{}", record["case"]);
        assert_samples_keep_pinned(&session, actor, 32);
    }
}
