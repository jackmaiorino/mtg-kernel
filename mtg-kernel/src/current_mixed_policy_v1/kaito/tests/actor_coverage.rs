//! Fixed decisions using the original issue #454 deck multisets, not RT1's
//! widened registrations. These scenes are coverage fixtures, not game results.
use super::*;
use crate::{
    ids::ObjectId,
    state::{GameState, Step, Zone},
};

#[test]
fn issue454_six_boolean_operands_are_explicit_and_digest_fields_are_absent() {
    // A normalization unit check, separate from the sealed engine fixtures
    // below. No synthetic JSON is admitted as executable authority.
    let encode = |flag: &str, value: bool, digest: &str| {
        let mut action = serde_json::Map::new();
        action.insert(flag.into(), value.into());
        action.insert("action_digest".into(), digest.into());
        super::super::super::normalize::encode(
            Source::KaitoV75,
            serde_json::json!({"observation": {}, "state_digest": digest}),
            vec![Value::Object(action)],
        )
        .unwrap()
    };
    for flag in [
        "pay",
        "change_target",
        "use_cost",
        "cast_it",
        "value",
        "include",
    ] {
        let no = encode(flag, false, "first-digest");
        let yes = encode(flag, true, "first-digest");
        assert_eq!(yes, encode(flag, true, "different-digest"));
        let differences = no.actions()[0]
            .rows()
            .iter()
            .zip(yes.actions()[0].rows())
            .filter(|(a, b)| a != b)
            .collect::<Vec<_>>();
        assert_eq!(
            differences.len(),
            1,
            "{flag} must be one explicit Boolean node"
        );
        assert_eq!(differences[0].0[0], 1);
        assert_eq!(differences[0].1[0], 1);
        assert_eq!(differences[0].0[1], 0);
        assert_eq!(differences[0].1[1], 1);
        assert_eq!(no.actions()[0].topology(), yes.actions()[0].topology());
    }
}

fn deck(text: &str) -> Vec<u16> {
    let value: Value = serde_json::from_str(text).unwrap();
    let cards = value["cards"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            let id = crate::card_def::card_id_by_name(row["name"].as_str().unwrap()).unwrap();
            std::iter::repeat_n(id, row["count"].as_u64().unwrap() as usize)
        })
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 40);
    cards
}

fn take_to_battlefield(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let card = crate::card_def::card_id_by_name(name).unwrap();
    let index = state.players[player.index()]
        .library
        .iter()
        .position(|&id| state.objects.get(id).card_def == card)
        .unwrap();
    // Initial scene only: move an existing registered object, never add a card
    // or manufacture a cast/activation/entry receipt.
    let id = state.players[player.index()].library.remove(index);
    let object = state.objects.get_mut(id);
    object.zone = Zone::Battlefield;
    object.summoning_sick = false;
    state.players[player.index()].battlefield.push(id);
    id
}

fn scene(player: PlayerId) -> (GameState, ObjectId, ObjectId) {
    let wr = deck(include_str!(
        "../../../../../data/limited/draftzero_sample_v1/decks/FDN_top_08530_WR.json"
    ));
    let rg = deck(include_str!(
        "../../../../../data/limited/draftzero_sample_v1/decks/FDN_top_26287_RG.json"
    ));
    let decks = if player == PlayerId::P0 {
        [wr, rg]
    } else {
        [rg, wr]
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &decks[0],
        &decks[1],
        crate::rl::card_name,
        454_08530,
        player,
    );
    state.step = Step::Main1;
    let krenko = take_to_battlefield(&mut state, player, "Krenko, Mob Boss");
    let blocker = take_to_battlefield(&mut state, player.opponent(), "Courageous Goblin");
    for owner in [PlayerId::P0, PlayerId::P1] {
        let mut actual = state.players[owner.index()]
            .library
            .iter()
            .chain(&state.players[owner.index()].battlefield)
            .map(|&id| state.objects.get(id).card_def)
            .collect::<Vec<_>>();
        let mut expected = decks[owner.index()].clone();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected, "original 40-card multiset changed");
    }
    (state, krenko, blocker)
}

#[test]
fn issue454_original_limited_krenko_every_action_scores_and_frozen_v4_refuses() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (state, krenko, _) = scene(player);
        let frozen_error = crate::rl::observe_policy_v6_unhashed_for_flat_policy(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            player,
            0,
            0,
            0,
            1,
        )
        .unwrap_err();
        assert!(
            frozen_error
                .to_string()
                .contains("Krenko requires selected V34 transport"),
            "{frozen_error}"
        );
        let native = FastActorSessionV1::from_kaito_v75_fixture_state(state);
        let FastActorResponseV1::Decision(h) = native.current_response() else {
            panic!(
                "Krenko fixture did not issue a decision: {:?}",
                native.current_response()
            );
        };
        assert!(native.flat_policy_observation_v4(h).is_err());
        let own = KaitoSession::from_session(native).unwrap();
        let decision = own.encode(h).unwrap();
        assert!(decision
            .backend
            .actions()
            .iter()
            .any(|action| matches!(action,
            A::ActivateAbility { source, ability_index: 0, .. } if source.arena_id == krenko.0)));
        let logits = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(decision.packet())
            .unwrap();
        assert_eq!(logits.len(), h.legal_action_count as usize);
        assert!(logits.iter().all(|score| score.is_finite()));
    }
}

#[test]
fn issue454_original_limited_attacker_and_blocker_include_tensors_differ() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let (state, krenko, blocker) = scene(player);
        let (mut own, mut native) = surveil_many::sessions(state);
        let mut seen = [false; 2];
        for _ in 0..64 {
            let FastActorResponseV1::Decision(h) = own.current_response() else {
                panic!("combat fixture halted before both inclusion decisions");
            };
            let decision = own.encode(h).unwrap();
            let actions = decision.backend.actions();
            let mut chosen = None;
            for (kind, object) in [krenko, blocker].into_iter().enumerate() {
                let matching = |action: &A, bit| match action {
                    A::ChooseAttackerInclusion {
                        attacker, include, ..
                    } => kind == 0 && attacker.arena_id == object.0 && *include == bit,
                    A::ChooseBlockerInclusion {
                        blocker, include, ..
                    } => kind == 1 && blocker.arena_id == object.0 && *include == bit,
                    _ => false,
                };
                if let Some(yes) = actions.iter().position(|a| matching(a, true)) {
                    let no = actions
                        .iter()
                        .position(|a| matching(a, false))
                        .expect("both include choices must be legal in this scene");
                    assert_ne!(
                        decision.packet().actions()[yes],
                        decision.packet().actions()[no]
                    );
                    assert_ne!(
                        decision.packet().actions()[yes].tensors(),
                        decision.packet().actions()[no].tensors()
                    );
                    seen[kind] = true;
                    chosen = Some(actions[yes].clone());
                }
            }
            let chosen = chosen
                .or_else(|| {
                    actions
                        .iter()
                        .find(|a| matches!(a, A::Pass { .. }))
                        .cloned()
                })
                .expect("fixture offered neither inclusion nor pass");
            act(&mut own, &mut native, |a| a == &chosen);
            if seen == [true; 2] {
                break;
            }
        }
        assert_eq!(seen, [true; 2]);
    }
}
