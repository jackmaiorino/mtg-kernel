use super::*;

#[test]
fn seeds_follow_sorted_compact_json_and_ignore_field_order() {
    let a = soft_q_seed_v1(&[
        ("case", json!("c")),
        ("role", json!("world")),
        ("ordinal", json!(3)),
    ]);
    let b = soft_q_seed_v1(&[
        ("ordinal", json!(3)),
        ("role", json!("world")),
        ("case", json!("c")),
    ]);
    assert_eq!(a, b);
    assert_eq!(a >> 63, 0, "masked to 63 bits");
    let expected = {
        let digest = Sha256::digest(br#"{"case":"c","ordinal":3,"role":"world"}"#);
        let mut first = [0_u8; 8];
        first.copy_from_slice(&digest[..8]);
        u64::from_le_bytes(first) & (u64::MAX >> 1)
    };
    assert_eq!(a, expected);
    assert_ne!(
        a,
        soft_q_seed_v1(&[
            ("case", json!("c")),
            ("role", json!("world")),
            ("ordinal", json!(4))
        ])
    );
    // Construction and assessment never share a stream.
    assert_ne!(
        case_seed(
            "c",
            SoftQDomainV1::Construction.world_role(),
            &[("ordinal", json!(0)), ("attempt", json!(0))]
        ),
        case_seed(
            "c",
            SoftQDomainV1::Assessment.world_role(),
            &[("ordinal", json!(0)), ("attempt", json!(0))]
        )
    );
}

#[test]
fn shuffle_is_a_deterministic_uniform_permutation() {
    for n in 2..=8 {
        let p = shuffle_permutation_v1("case-a", n);
        let mut sorted = p.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..n).collect::<Vec<_>>());
        assert_eq!(p, shuffle_permutation_v1("case-a", n));
    }
    // Across many cases every position takes every value (a uniform draw,
    // identity included).
    let mut seen = [[false; 4]; 4];
    let mut identity = 0;
    for i in 0..400 {
        let p = shuffle_permutation_v1(&format!("case-{i}"), 4);
        identity += usize::from(p == [0, 1, 2, 3]);
        for (pos, &v) in p.iter().enumerate() {
            seen[pos][v] = true;
        }
    }
    assert!(seen.iter().flatten().all(|&b| b));
    assert!((5..=35).contains(&identity), "identity count {identity}");
}

fn ordinal(returns: Vec<Option<i8>>) -> SoftQOrdinalV1 {
    SoftQOrdinalV1 {
        ordinal: 0,
        rejections: vec![],
        world_attempt: Some(0),
        prior_deck: Some(0),
        returns,
        policy_steps: 0,
    }
}

#[test]
fn targets_are_all_or_nothing_and_anchor_on_the_logits() {
    let logits = [0.5_f32, -1.0, 2.0];
    let full = |r: [i8; 3]| -> Vec<SoftQOrdinalV1> {
        (0..16)
            .map(|k| {
                let mut o = ordinal(r.iter().map(|&v| Some(v)).collect());
                o.ordinal = k;
                o
            })
            .collect()
    };
    // Tied returns reproduce the collection softmax.
    let tied = root_target_v1(&full([1, 1, 1]), &logits, SoftQArmV1::Treatment, "c").unwrap();
    assert_eq!(tied.status, "available");
    let z: Vec<f64> = logits.iter().map(|&v| f64::from(v)).collect();
    let max = z.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = z.iter().map(|v| (v - max).exp()).sum();
    for (q, v) in tied.target.as_ref().unwrap().iter().zip(&z) {
        assert!((q - (v - max).exp() / sum).abs() < 1e-12);
    }
    // Returns shift log-odds by (Q_i - Q_j) / 0.25.
    let mut ords = full([1, -1, -1]);
    ords[0].returns = vec![Some(1), Some(1), Some(-1)];
    let t = root_target_v1(&ords, &logits, SoftQArmV1::Treatment, "c").unwrap();
    let q = t.target.as_ref().unwrap();
    let m = t.mean_returns.as_ref().unwrap();
    assert_eq!(m[0], 1.0);
    assert_eq!(m[1], (1.0 - 15.0) / 16.0);
    let expected = (z[0] - z[1]) + (m[0] - m[1]) / SOFT_Q_TEMPERATURE_V1;
    assert!(((q[0] / q[1]).ln() - expected).abs() < 1e-9);
    // One censored rollout or one ordinal without a world: unavailable.
    let mut censored = full([1, 0, -1]);
    censored[7].returns[2] = None;
    assert_eq!(
        root_target_v1(&censored, &logits, SoftQArmV1::Treatment, "c")
            .unwrap()
            .status,
        "censored"
    );
    let mut rejected = full([1, 0, -1]);
    rejected[3].world_attempt = None;
    rejected[3].returns.clear();
    assert_eq!(
        root_target_v1(&rejected, &logits, SoftQArmV1::Treatment, "c")
            .unwrap()
            .status,
        "rejected"
    );
    assert!(root_target_v1(&full([1, 0, -1])[..15], &logits, SoftQArmV1::Treatment, "c").is_err());
}

#[test]
fn the_shuffled_arm_permutes_q_but_keeps_its_own_logits() {
    let logits = [0.0_f32, 0.0, 0.0, 0.0];
    let ords: Vec<SoftQOrdinalV1> = (0..16)
        .map(|_| ordinal(vec![Some(1), Some(0), Some(-1), Some(-1)]))
        .collect();
    let treated = root_target_v1(&ords, &logits, SoftQArmV1::Treatment, "case-x").unwrap();
    let shuffled = root_target_v1(&ords, &logits, SoftQArmV1::ShuffledQ, "case-x").unwrap();
    let p = shuffled.permutation.clone().unwrap();
    assert_eq!(p, shuffle_permutation_v1("case-x", 4));
    assert_eq!(treated.mean_returns, shuffled.mean_returns);
    let tq = treated.target.unwrap();
    let sq = shuffled.target.unwrap();
    for (i, &j) in p.iter().enumerate() {
        // With equal logits, shuffled q at i is treated q at p[i].
        assert!((sq[i] - tq[j]).abs() < 1e-12);
    }
}

#[test]
fn deployed_masses_sum_to_two_to_the_sixty_four() {
    let masses = deployed_masses_v1(&[0.3, -2.0, 1.1, 0.0]).unwrap();
    let total: u128 = masses.iter().map(|m| m.parse::<u128>().unwrap()).sum();
    assert_eq!(total, 1_u128 << 64);
}

fn list(id: &str) -> ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    let cards = registration.registered_configuration();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

/// The world depends on what the actor can see, not on the opponent's true
/// cards: replacing every card of the opponent's hidden hand and library
/// (identities only, same objects and order) leaves every sampled world
/// unchanged.
#[test]
fn worlds_do_not_read_the_true_opponent_deck() {
    let prior = world::DeckPrior::new(&(0..9).collect::<Vec<_>>());
    let decks = [list("Burn"), list("Affinity")];
    let episode = ExpandedEpisodeV1 {
        id: "sq1-invariance".into(),
        seed: 2_026_101_801,
        starting_player: 0,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: decks.clone(),
        selected: decks.clone(),
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
    };
    let original = episode_session_v1(&episode, &episode.configurations().unwrap()).unwrap();
    let FastActorResponseV1::Decision(d) = original.current_response() else {
        panic!("a live first decision")
    };
    let opponent = 1 - usize::from(seat(d.acting_player));
    let forest = crate::card_def::CARD_DEFS
        .iter()
        .position(|c| c.name == "Forest")
        .unwrap() as u16;
    let changed = original.census_edited_clone_v1(|st| {
        let hidden: Vec<_> = st.players[opponent]
            .library
            .iter()
            .chain(st.players[opponent].hand.iter())
            .copied()
            .collect();
        for id in hidden {
            // As the prior's own hook replaces an identity (stage4a world.rs).
            let obj = st.objects.get_mut(id);
            obj.card_def = forest;
            obj.name = crate::card_def::CARD_DEFS[forest as usize].name.to_string();
            obj.v4 = crate::state::ObjectStateV4::from_card_def(forest);
        }
    });
    assert_ne!(
        original.diagnostic_state_hash(),
        changed.diagnostic_state_hash()
    );
    let mut compared = 0;
    for seed in 0..24_u64 {
        match (
            world::sample(&original, seed, &prior),
            world::sample(&changed, seed, &prior),
        ) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.prior_deck, b.prior_deck);
                assert_eq!(
                    a.world.diagnostic_state_hash(),
                    b.world.diagnostic_state_hash(),
                    "seed {seed}"
                );
                compared += 1;
            }
            (Err(x), Err(y)) => assert_eq!(x, y),
            (x, y) => panic!("seed {seed}: {:?} vs {:?}", x.err(), y.err()),
        }
    }
    assert!(compared >= 12, "only {compared} accepted samples");
}

#[test]
fn sampler_faults_are_not_rejections() {
    for kind in SAMPLER_REJECTIONS_V1 {
        assert!(sampler_rejection_v1(kind));
        assert!(sampler_rejection_v1(&format!("{kind}: hook detail")));
    }
    for fault in [
        "InvalidVisibleBinding",
        "HiddenStateContract",
        "NoLiveDecision",
        "UnsupportedActionContract",
        "StepFailed",
        "HaltedSimulation",
        // Pending-choice validation and stale-origin binding failures.
        "LibraryChoicePlanFailed",
        "LibraryChoiceOriginFailed: detail",
        "deck prior hook did not run",
        "",
    ] {
        assert!(!sampler_rejection_v1(fault), "{fault}");
    }
}
