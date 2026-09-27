//! Information boundary of the D3 search on the opponent seat.
//!
//! The searcher's root and every simulated world must be a function of its
//! own information set. Each variant differs from the original root only in
//! state the searcher cannot observe: which unseen learner cards sit in hand
//! versus library, the learner's or the searcher's own unobserved library
//! order, or the environment's future random stream. The unchanged D3
//! wrapper must return the same action and byte-identical decision records,
//! and every resampled world must be identical. Two power checks show that
//! the comparison is not vacuous: visible state changes the search, and the
//! Legacy sample mode, which keeps the true random stream, is caught.
//!
//! Scope: the sampler reshuffles the learner's true unseen cards, so the
//! search does know the learner's remaining card multiset (D3's reviewed
//! known-decklist convention). These tests do not claim invariance to it.
use super::{REVIEWED_DESCRIPTOR_JSON, REVIEWED_DESCRIPTOR_SHA256};
use crate::environment_randomization_v2::{GameEnvironmentRandomizationV2, PhysicalOwnerV2};
use crate::ids::PlayerId;
use crate::learned_bo3_v1::public_evaluation::search_v3::SearchPlayV3;
use crate::paired_bo1_harness_v1::{PairedBo1PolicyInputV1, PairedBo1PolicyV1};
use crate::phase1_agent_v1::V4InformationSetSearchDescriptorV1;
use crate::policy_observation_v6::tests::{forest_search_state, put, ready_state};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, V4SearchSampleMode,
};
use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
use crate::state::{GameState, Zone};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Arbitrary sampler seeds, including both ends of the u64 range.
const SEEDS: [u64; 8] = [
    0,
    1,
    81,
    8172,
    20260922,
    0x5a5a_a5a5_1248_8421,
    u64::MAX - 1,
    u64::MAX,
];

/// The reviewed descriptor with only its model identity replaced by the
/// fixture policy's. Budget, seed, schema and algorithm stay byte-for-byte.
fn fixture_descriptor(policy: &FrozenPlayPolicyV1) -> V4InformationSetSearchDescriptorV1 {
    let mut d: V4InformationSetSearchDescriptorV1 =
        serde_json::from_str(REVIEWED_DESCRIPTOR_JSON).unwrap();
    let m = policy.actual_model_identity_v1();
    d.weights_sha256 = m.weights_sha256;
    d.model_parameter_sha256 = m.model_parameter_sha256;
    d.embedding_table_sha256 = m.embedding_table_sha256;
    d.feature_contract_digest = m.feature_contract_digest;
    d.feature_encoding_digest = m.feature_encoding_digest;
    d
}

fn session(state: GameState) -> FastActorSessionV1 {
    FastActorSessionV1::from_v3_fixture_state(state)
}

fn decision(s: &FastActorSessionV1) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        panic!("fixture has no live decision");
    };
    d
}

/// The searcher holds Lightning Bolt with three red mana. The learner holds
/// two unseen cards and both libraries hold five unseen cards, so every
/// hidden channel below has something to move.
fn root_state(searcher: PlayerId) -> GameState {
    let mut s = ready_state();
    s.active_player = searcher;
    s.priority_player = searcher;
    put(&mut s, searcher, "Lightning Bolt", Zone::Hand);
    s.players[searcher.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 3;
    add_learner_cards(&mut s, searcher.opponent());
    for name in ["Forest", "Mountain", "Island", "Swamp", "Counterspell"] {
        put(&mut s, searcher, name, Zone::Library);
    }
    s
}

fn add_learner_cards(s: &mut GameState, learner: PlayerId) {
    put(s, learner, "Gut Shot", Zone::Hand);
    put(s, learner, "Lotus Petal", Zone::Hand);
    for name in ["Forest", "Mountain", "Island", "Swamp", "Counterspell"] {
        put(s, learner, name, Zone::Library);
    }
}

/// Replace the whole random stream, legacy or environment-v2. Past
/// physical-owner shuffle counters are public and stay fixed.
fn with_randomness(state: &GameState, environment: bool, seed: u64) -> GameState {
    let mut value = serde_json::to_value(state).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("rng");
    object.remove("environment_randomization_v2");
    if environment {
        let mut rng = GameEnvironmentRandomizationV2::new(seed);
        rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P0, 4);
        rng.set_live_shuffle_ordinal(PhysicalOwnerV2::P1, 9);
        object.insert(
            "environment_randomization_v2".into(),
            serde_json::to_value(rng).unwrap(),
        );
    } else {
        object.insert("rng".into(), serde_json::json!({ "state": seed }));
    }
    serde_json::from_value(value).unwrap()
}

/// Exchange the owner's first hand card with a library card of another
/// identity. Hand size, library size and every visible zone are unchanged.
fn swap_hand_and_library(mut s: GameState, owner: PlayerId) -> GameState {
    let p = owner.index();
    let hand = s.players[p].hand[0];
    let definition = s.objects.get(hand).card_def;
    let slot = s.players[p]
        .library
        .iter()
        .position(|id| s.objects.get(*id).card_def != definition)
        .expect("fixture library has another identity");
    let library = s.players[p].library[slot];
    s.players[p].hand[0] = library;
    s.players[p].library[slot] = hand;
    s.objects.get_mut(library).zone = Zone::Hand;
    s.objects.get_mut(hand).zone = Zone::Library;
    s
}

fn reverse_library(mut s: GameState, owner: PlayerId) -> GameState {
    s.players[owner.index()].library.reverse();
    s
}

/// Full-state hashes of the resampled world for each fixed seed.
fn worlds(s: &FastActorSessionV1, mode: V4SearchSampleMode) -> Vec<u64> {
    SEEDS
        .iter()
        .map(|seed| {
            s.kernel_search_redeterminized_clone_mode_v4(*seed, mode)
                .unwrap()
                .diagnostic_state_hash()
        })
        .collect()
}

/// One decision of the unchanged D3 wrapper: the chosen action and the
/// wrapper's records (visible binding, root key, estimates, visits, priors,
/// work census and outcome SHA256).
fn d3(s: &FastActorSessionV1) -> (u32, Value) {
    let policy = FrozenPlayPolicyV1::training_fixture_v4();
    let descriptor = fixture_descriptor(&policy);
    let mut wrapper = SearchPlayV3::new(policy, descriptor).unwrap();
    wrapper.begin_match();
    wrapper.reset_for_game_v1([123, 456]).unwrap();
    let selected = wrapper
        .select_action_v1(PairedBo1PolicyInputV1::new(s, decision(s)))
        .unwrap();
    (selected, wrapper.records())
}

fn assert_same_information_set(
    context: &str,
    original: &FastActorSessionV1,
    variant: &FastActorSessionV1,
    expected: &(u32, Value),
) {
    assert_ne!(
        variant.diagnostic_state_hash(),
        original.diagnostic_state_hash(),
        "{context}: perturbation changed nothing"
    );
    assert_eq!(decision(variant), decision(original), "{context}: decision");
    assert_eq!(
        variant.kernel_search_visible_key_v4(8).unwrap(),
        original.kernel_search_visible_key_v4(8).unwrap(),
        "{context}: visible root key"
    );
    assert_eq!(
        worlds(variant, V4SearchSampleMode::FutureChanceV3),
        worlds(original, V4SearchSampleMode::FutureChanceV3),
        "{context}: resampled worlds"
    );
    assert_eq!(&d3(variant), expected, "{context}: D3 action and records");
}

#[test]
fn reviewed_descriptor_copy_is_exact_and_accepted_by_the_wrapper() {
    let digest = format!("{:x}", Sha256::digest(REVIEWED_DESCRIPTOR_JSON.as_bytes()));
    assert_eq!(digest, REVIEWED_DESCRIPTOR_SHA256);
    let policy = FrozenPlayPolicyV1::training_fixture_v4();
    let descriptor = fixture_descriptor(&policy);
    assert_eq!(
        (
            descriptor.simulations,
            descriptor.transitions,
            descriptor.depth,
            descriptor.experiment_seed
        ),
        (128, 1024, 8, 20260922)
    );
    SearchPlayV3::new(policy, descriptor).unwrap();
    // The reviewed bytes name g115, not the fixture, so they must be refused here.
    let reviewed: V4InformationSetSearchDescriptorV1 =
        serde_json::from_str(REVIEWED_DESCRIPTOR_JSON).unwrap();
    assert!(SearchPlayV3::new(FrozenPlayPolicyV1::training_fixture_v4(), reviewed).is_err());
}

#[test]
fn d3_opponent_ignores_learner_hand_library_order_and_future_randomness() {
    for searcher in [PlayerId::P0, PlayerId::P1] {
        for environment in [false, true] {
            let learner = searcher.opponent();
            let base = with_randomness(&root_state(searcher), environment, 111);
            let original = session(base.clone());
            let expected = d3(&original);
            let variants = [
                (
                    "learner hand versus library",
                    swap_hand_and_library(base.clone(), learner),
                ),
                (
                    "learner library order",
                    reverse_library(base.clone(), learner),
                ),
                (
                    "searcher library order",
                    reverse_library(base.clone(), searcher),
                ),
                (
                    "future randomness",
                    with_randomness(&base, environment, 999),
                ),
            ];
            for (name, state) in variants {
                let context = format!("{name}, searcher {searcher:?}, environment {environment}");
                assert_same_information_set(&context, &original, &session(state), &expected);
            }
        }
    }
}

#[test]
fn d3_opponent_library_choice_root_ignores_learner_hidden_state() {
    for environment in [false, true] {
        let mut state = forest_search_state(false, "Lightning Bolt");
        add_learner_cards(&mut state, PlayerId::P1);
        let base = with_randomness(&state, environment, 111);
        let original = session(base.clone());
        assert_eq!(
            decision(&original).acting_player,
            crate::rl::PlayerSeatV1::P0,
            "fixture searcher"
        );
        let expected = d3(&original);
        let variants = [
            (
                "learner hand versus library",
                swap_hand_and_library(base.clone(), PlayerId::P1),
            ),
            (
                "learner library order",
                reverse_library(base.clone(), PlayerId::P1),
            ),
            (
                "future randomness",
                with_randomness(&base, environment, 999),
            ),
        ];
        for (name, state) in variants {
            let context = format!("library choice, {name}, environment {environment}");
            assert_same_information_set(&context, &original, &session(state), &expected);
        }
    }
}

/// Power check: the Legacy sample mode copies the true random stream into
/// every resampled world, and the world comparison catches it. D3's
/// FutureChanceV3 mode is what removes that channel.
#[test]
fn legacy_sample_mode_keeps_the_true_stream_and_is_caught() {
    for searcher in [PlayerId::P0, PlayerId::P1] {
        for environment in [false, true] {
            let base = with_randomness(&root_state(searcher), environment, 111);
            let original = session(base.clone());
            let variant = session(with_randomness(&base, environment, 999));
            let legacy = |s: &FastActorSessionV1| worlds(s, V4SearchSampleMode::Legacy);
            let (a, b) = (legacy(&original), legacy(&variant));
            assert!(
                a.iter().zip(&b).all(|(x, y)| x != y),
                "Legacy worlds hide the real stream"
            );
            assert_eq!(
                worlds(&original, V4SearchSampleMode::FutureChanceV3),
                worlds(&variant, V4SearchSampleMode::FutureChanceV3)
            );
        }
    }
}

/// Power check: state the searcher can see changes its estimates, so the
/// identical records above are not an artifact of a search that ignores its
/// input. At 3 life the learner is within Lightning Bolt range.
#[test]
fn d3_opponent_responds_to_visible_state() {
    for searcher in [PlayerId::P0, PlayerId::P1] {
        let base = root_state(searcher);
        let mut lethal = base.clone();
        lethal.players[searcher.opponent().index()].life = 3;
        let (original, lethal) = (session(base), session(lethal));
        assert_ne!(
            original.kernel_search_visible_key_v4(8).unwrap(),
            lethal.kernel_search_visible_key_v4(8).unwrap()
        );
        let estimates = |s: &FastActorSessionV1| d3(s).1["decisions"][0]["root_estimates"].clone();
        assert_ne!(
            estimates(&original),
            estimates(&lethal),
            "searcher {searcher:?}"
        );
    }
}
