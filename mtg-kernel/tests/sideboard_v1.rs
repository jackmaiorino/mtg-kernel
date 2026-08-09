use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::sideboard::{
    checked_in_pauper_registered_deck_by_id_v1, checked_in_pauper_registered_decks_v1, CardCountV1,
    DeterministicSideboardPolicyV1, RegisteredDeckV1, SideboardDefaultPlanV1, SideboardErrorV1,
    SideboardPlanV1, SideboardZoneV1, REGISTERED_DECK_SIZE_V1,
};

fn card_id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name} in CARD_DEFS"))
}

fn registered_a_v1() -> RegisteredDeckV1 {
    let mut mainboard = vec![10; 4];
    mainboard.extend(std::iter::repeat_n(11, 56));
    let mut sideboard = vec![20; 3];
    sideboard.extend(std::iter::repeat_n(21, 12));
    RegisteredDeckV1::new_exact_v1("A", mainboard, sideboard).unwrap()
}

fn exchange_plan_v1(game_index: u8) -> SideboardPlanV1 {
    SideboardPlanV1::new_v1(
        "A",
        "B",
        game_index,
        vec![CardCountV1 {
            card_id: 20,
            count: 3,
        }],
        vec![CardCountV1 {
            card_id: 10,
            count: 3,
        }],
    )
    .unwrap()
}

#[test]
fn exact_registration_is_canonical_and_rejects_wrong_zone_sizes() {
    let ordered =
        RegisteredDeckV1::new_exact_v1("Deck", (0..60).collect(), (60..75).collect()).unwrap();
    let reversed =
        RegisteredDeckV1::new_exact_v1("Deck", (0..60).rev().collect(), (60..75).rev().collect())
            .unwrap();
    assert_eq!(ordered, reversed);
    assert_eq!(
        ordered.registered_75_sha256_v1(),
        reversed.registered_75_sha256_v1()
    );
    assert_eq!(
        ordered
            .registered_configuration()
            .combined_card_counts_v1()
            .iter()
            .map(|row| usize::from(row.count))
            .sum::<usize>(),
        REGISTERED_DECK_SIZE_V1
    );

    assert_eq!(
        RegisteredDeckV1::new_exact_v1("Deck", vec![1; 59], vec![2; 15]),
        Err(SideboardErrorV1::WrongMainboardSize { actual: 59 })
    );
    assert_eq!(
        RegisteredDeckV1::new_exact_v1("Deck", vec![1; 60], vec![2; 14]),
        Err(SideboardErrorV1::WrongSideboardSize { actual: 14 })
    );
}

#[test]
fn executable_registration_rejects_unknown_token_and_unsupported_cards() {
    let mountain = card_id("Mountain");
    let island = card_id("Island");
    let admitted =
        RegisteredDeckV1::new_fully_supported_v1("Deck", vec![mountain; 60], vec![island; 15])
            .unwrap();
    assert_eq!(admitted.registered_configuration().mainboard().len(), 60);
    assert_eq!(admitted.registered_configuration().sideboard().len(), 15);

    assert_eq!(
        RegisteredDeckV1::new_fully_supported_v1("Deck", vec![mountain; 60], vec![u16::MAX; 15],),
        Err(SideboardErrorV1::UnknownRegisteredCardId {
            zone: SideboardZoneV1::RegisteredSideboard,
            card_id: u16::MAX,
        })
    );

    let blood = card_id("Blood Token");
    assert_eq!(
        RegisteredDeckV1::new_fully_supported_v1("Deck", vec![mountain; 60], vec![blood; 15]),
        Err(SideboardErrorV1::TokenInRegisteredDeck {
            zone: SideboardZoneV1::RegisteredSideboard,
            card_id: blood,
            card_name: "Blood Token".to_owned(),
        })
    );

    if let Some((unsupported, definition)) = CARD_DEFS
        .iter()
        .enumerate()
        .find(|(_, definition)| !definition.is_token && !definition.has_full_support())
    {
        let unsupported = u16::try_from(unsupported).unwrap();
        assert_eq!(
            RegisteredDeckV1::new_fully_supported_v1(
                "Deck",
                vec![unsupported; 60],
                vec![island; 15],
            ),
            Err(SideboardErrorV1::CardNotFullySupported {
                zone: SideboardZoneV1::RegisteredMainboard,
                card_id: unsupported,
                card_name: definition.name.to_owned(),
            })
        );
    }
}

#[test]
fn checked_in_pool_is_wired_to_the_executable_bo3_admission_gate() {
    match checked_in_pauper_registered_decks_v1() {
        Ok(decks) => {
            assert_eq!(decks.len(), 9);
            assert!(decks.iter().all(|deck| {
                deck.registered_configuration().mainboard().len() == 60
                    && deck.registered_configuration().sideboard().len() == 15
            }));
        }
        Err(SideboardErrorV1::CardNotFullySupported { .. })
        | Err(SideboardErrorV1::UnregisteredPoolCard { .. }) => {
            // This is the intended current outcome until the remaining pool
            // cards are promoted. No structural or policy error is accepted.
        }
        Err(other) => panic!("unexpected checked-in pool admission error: {other}"),
    }
}

#[test]
fn completed_checked_in_decks_are_individually_admitted_for_bo3() {
    for deck_id in ["Rally", "Burn", "Terror"] {
        let deck = checked_in_pauper_registered_deck_by_id_v1(deck_id).unwrap();
        assert_eq!(deck.deck_id(), deck_id);
        assert_eq!(deck.registered_configuration().mainboard().len(), 60);
        assert_eq!(deck.registered_configuration().sideboard().len(), 15);
    }

    assert_eq!(
        checked_in_pauper_registered_deck_by_id_v1("Missing"),
        Err(SideboardErrorV1::UnknownPoolDeckId {
            deck_id: "Missing".to_owned(),
        })
    );
}

#[test]
fn legal_exchange_preserves_registered_75_and_has_canonical_receipt() {
    let plan = exchange_plan_v1(2);
    let policy = DeterministicSideboardPolicyV1::new_v1(
        "test-policy/v1",
        vec!["B".to_owned(), "A".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![plan],
    )
    .unwrap();
    let registered = registered_a_v1();
    let (configured, receipt) = policy.apply_v1(&registered, "B", 2).unwrap();

    assert_eq!(configured.mainboard().len(), 60);
    assert_eq!(configured.sideboard().len(), 15);
    assert_eq!(
        configured
            .mainboard()
            .iter()
            .filter(|id| **id == 10)
            .count(),
        1
    );
    assert_eq!(
        configured
            .mainboard()
            .iter()
            .filter(|id| **id == 20)
            .count(),
        3
    );
    assert_eq!(
        configured
            .sideboard()
            .iter()
            .filter(|id| **id == 10)
            .count(),
        3
    );
    assert_eq!(
        configured
            .sideboard()
            .iter()
            .filter(|id| **id == 20)
            .count(),
        0
    );
    assert_eq!(
        configured.combined_card_counts_v1(),
        registered
            .registered_configuration()
            .combined_card_counts_v1()
    );

    assert_eq!(receipt.policy_id(), policy.policy_id());
    assert_eq!(receipt.schema(), "kernel_sideboard_receipt/v1");
    assert_eq!(receipt.policy_sha256(), policy.policy_sha256_v1());
    assert_eq!(
        receipt.registered_75_sha256(),
        registered.registered_75_sha256_v1()
    );
    assert_eq!(receipt.game_index(), 2);
    assert_eq!(
        receipt.cards_in(),
        &[CardCountV1 {
            card_id: 20,
            count: 3
        }]
    );
    assert_eq!(
        receipt.cards_out(),
        &[CardCountV1 {
            card_id: 10,
            count: 3
        }]
    );
    assert_eq!(
        receipt.after_mainboard_sha256(),
        configured.mainboard_sha256_v1()
    );
    assert_eq!(
        receipt.after_sideboard_sha256(),
        configured.sideboard_sha256_v1()
    );

    let (_, repeated_receipt) = policy.apply_v1(&registered, "B", 2).unwrap();
    assert_eq!(
        receipt.canonical_bytes_v1(),
        repeated_receipt.canonical_bytes_v1()
    );
    assert_eq!(
        receipt.receipt_sha256_v1(),
        repeated_receipt.receipt_sha256_v1()
    );
}

#[test]
fn game_index_plans_are_resolved_from_registration_not_cumulatively() {
    let policy = DeterministicSideboardPolicyV1::new_v1(
        "test-policy/v1",
        vec!["A".to_owned(), "B".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![exchange_plan_v1(2)],
    )
    .unwrap();
    let registered = registered_a_v1();

    let (game_two, _) = policy.apply_v1(&registered, "B", 2).unwrap();
    assert_ne!(game_two, *registered.registered_configuration());

    let (game_three, receipt) = policy.apply_v1(&registered, "B", 3).unwrap();
    assert_eq!(game_three, *registered.registered_configuration());
    assert!(receipt.cards_in().is_empty());
    assert!(receipt.cards_out().is_empty());
}

#[test]
fn exchange_validation_fails_closed_before_deck_mutation() {
    assert_eq!(
        SideboardPlanV1::new_v1(
            "A",
            "B",
            2,
            vec![CardCountV1 {
                card_id: 20,
                count: 2
            }],
            vec![CardCountV1 {
                card_id: 10,
                count: 1
            }],
        ),
        Err(SideboardErrorV1::UnequalExchangeCounts {
            cards_in: 2,
            cards_out: 1,
        })
    );
    assert_eq!(
        SideboardPlanV1::keep_registered_v1("A", "B", 1),
        Err(SideboardErrorV1::InvalidPostboardGameIndex { actual: 1 })
    );
    assert_eq!(
        SideboardPlanV1::keep_registered_v1("A", "B", 4)
            .unwrap()
            .game_index(),
        4
    );
    assert_eq!(
        SideboardPlanV1::new_v1(
            "A",
            "B",
            2,
            vec![CardCountV1 {
                card_id: 20,
                count: 1
            }],
            vec![CardCountV1 {
                card_id: 20,
                count: 1
            }],
        ),
        Err(SideboardErrorV1::CardListedBothInAndOut { card_id: 20 })
    );

    let registered = registered_a_v1();
    let unavailable = SideboardPlanV1::new_v1(
        "A",
        "B",
        2,
        vec![CardCountV1 {
            card_id: 99,
            count: 1,
        }],
        vec![CardCountV1 {
            card_id: 10,
            count: 1,
        }],
    )
    .unwrap();
    assert_eq!(
        registered.apply_plan_v1(&unavailable, "test-policy/v1", [7; 32]),
        Err(SideboardErrorV1::CardUnavailable {
            zone: SideboardZoneV1::RegisteredSideboard,
            card_id: 99,
            requested: 1,
            available: 0,
        })
    );
    assert_eq!(registered.registered_configuration().mainboard().len(), 60);
}

#[test]
fn policy_hash_and_plan_selection_are_order_independent_and_keyed() {
    let plan_two = exchange_plan_v1(2);
    let plan_three = exchange_plan_v1(3);
    let left = DeterministicSideboardPolicyV1::new_v1(
        "test-policy/v1",
        vec!["B".to_owned(), "A".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![plan_three.clone(), plan_two.clone()],
    )
    .unwrap();
    let right = DeterministicSideboardPolicyV1::new_v1(
        "test-policy/v1",
        vec!["A".to_owned(), "B".to_owned()],
        SideboardDefaultPlanV1::KeepRegisteredConfiguration,
        vec![plan_two, plan_three],
    )
    .unwrap();
    assert_eq!(left.canonical_bytes_v1(), right.canonical_bytes_v1());
    assert_eq!(left.policy_sha256_v1(), right.policy_sha256_v1());
    assert_eq!(
        left.plan_for_v1("A", "B", 2).unwrap().cards_in()[0].card_id,
        20
    );
    assert_eq!(
        left.plan_for_v1("B", "A", 2).unwrap(),
        SideboardPlanV1::keep_registered_v1("B", "A", 2).unwrap()
    );
    assert_eq!(
        left.plan_for_v1("missing", "B", 2),
        Err(SideboardErrorV1::UnknownPolicyDeckId {
            deck_id: "missing".to_owned(),
        })
    );

    assert!(matches!(
        DeterministicSideboardPolicyV1::new_v1(
            "test-policy/v1",
            vec!["A".to_owned(), "B".to_owned()],
            SideboardDefaultPlanV1::KeepRegisteredConfiguration,
            vec![exchange_plan_v1(2), exchange_plan_v1(2)],
        ),
        Err(SideboardErrorV1::DuplicatePlanKey { .. })
    ));
}

#[test]
fn checked_in_policy_is_strict_versioned_and_covers_nine_decks() {
    let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1().unwrap();
    assert_eq!(policy.policy_id(), "pauper-registered-75-static/v1");
    assert_eq!(policy.deck_ids().len(), 9);
    assert_eq!(policy.deck_ids().first().unwrap(), "Affinity");
    assert_eq!(policy.deck_ids().last().unwrap(), "Wildfire");
    assert!(policy.plans().is_empty());

    let registered = RegisteredDeckV1::new_exact_v1("Rally", vec![1; 60], vec![2; 15]).unwrap();
    let (configured, receipt) = policy.apply_v1(&registered, "Burn", 2).unwrap();
    assert_eq!(configured, *registered.registered_configuration());
    assert!(receipt.cards_in().is_empty());
    assert_ne!(policy.policy_sha256_v1(), [0; 32]);

    let (after_draws, draw_receipt) = policy.apply_v1(&registered, "Burn", 4).unwrap();
    assert_eq!(after_draws, *registered.registered_configuration());
    assert_eq!(draw_receipt.game_index(), 4);
    assert!(draw_receipt.cards_in().is_empty());

    let unknown_field = mtg_kernel::sideboard::PAUPER_SIDEBOARD_POLICY_JSON_V1.replacen(
        "\"schema\":",
        "\"unexpected\": true, \"schema\":",
        1,
    );
    assert!(matches!(
        DeterministicSideboardPolicyV1::from_json_v1(&unknown_field),
        Err(SideboardErrorV1::PolicyJson(_))
    ));

    let incomplete_coverage = mtg_kernel::sideboard::PAUPER_SIDEBOARD_POLICY_JSON_V1.replacen(
        "\"postboard_game_index_minimum\": 2",
        "\"postboard_game_index_minimum\": 3",
        1,
    );
    assert_eq!(
        DeterministicSideboardPolicyV1::from_json_v1(&incomplete_coverage),
        Err(SideboardErrorV1::PolicyCoverageMismatch)
    );
}
